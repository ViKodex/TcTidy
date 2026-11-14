using System;
using System.Collections.Generic;
using System.Threading.Tasks;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// Formats POUs that are open in the editor, in three steps. On the caller's thread (the UI
    /// thread, which the POUs require), it flushes each POU's dirty buffer so it reads what is on
    /// screen (because it always reads the on-screen text, the data-loss path where unrelated unsaved
    /// edits were overwritten is closed by construction) and reads every ST section. Off that thread,
    /// so the IDE stays responsive, one tctidy run formats all the sections. Back on the caller's
    /// thread, it checks that each POU still holds the text that was formatted, then applies all
    /// writes inside a per-POU transaction. A POU edited in the meantime is left untouched. A failed
    /// write, save, or commit restores every attempted section and aborts the transaction. A
    /// <see cref="FormatterException"/> is recorded as a per-POU failure, while unrelated POUs continue.
    /// Pure: no Visual Studio or COM dependency, so it is unit-tested without an IDE or TwinCAT.
    /// </summary>
    public sealed class OpenPouFormatter
    {
        private readonly ITctidyRunner _runner;

        public OpenPouFormatter(ITctidyRunner runner)
        {
            _runner = runner ?? throw new ArgumentNullException(nameof(runner));
        }

        /// <summary>
        /// Call on the thread the POUs belong to, with a synchronization context that returns to
        /// it: the await resumes there (no <c>ConfigureAwait(false)</c>).
        /// </summary>
        public async Task<BatchOutcome> FormatAsync(IReadOnlyList<IOpenPou> pous)
        {
            if (pous == null) throw new ArgumentNullException(nameof(pous));

            // One slot per POU, so the failures stay in the POUs' order whichever step they come from.
            var failures = new FileFailure?[pous.Count];
            var captured = new CapturedPou?[pous.Count];
            var requests = new List<SectionRequest>();
            for (var i = 0; i < pous.Count; i++)
            {
                var (pou, slot) = (pous[i], i);
                failures[i] = Guard(pou, () => captured[slot] = Capture(pou, requests));
            }

            IReadOnlyList<SectionResult> results = Array.Empty<SectionResult>();
            if (requests.Count > 0)
            {
                try
                {
                    results = await Task.Run(() => _runner.FormatSections(requests));
                    if (results.Count != requests.Count)
                        throw new FormatterException(-1, "tctidy did not return one result per section");
                }
                catch (FormatterException ex)
                {
                    // No results at all: nothing was formatted, so nothing is written.
                    for (var i = 0; i < pous.Count; i++)
                    {
                        if (captured[i] != null) failures[i] = new FileFailure(pous[i].Name, ex.Message);
                    }
                    return new BatchOutcome(0, 0, Compact(failures));
                }
            }

            var (changed, unchanged) = (0, 0);
            for (var i = 0; i < pous.Count; i++)
            {
                if (captured[i] is not { } read) continue;
                failures[i] = Guard(read.Pou, () =>
                {
                    if (Apply(read, results)) changed++;
                    else unchanged++;
                });
            }

            return new BatchOutcome(changed, unchanged, Compact(failures));
        }

        // Runs one POU's step, turning its failure into the POU's result so the batch goes on.
        private static FileFailure? Guard(IOpenPou pou, Action step)
        {
            try
            {
                step();
                return null;
            }
            catch (ElementFailure ex)
            {
                // Source is the offending element's name (the POU, or the specific method/action);
                // the diagnostic stays separate so the report can surface format and Automation
                // Interface failures per element, rather than naming the parent POU only.
                return new FileFailure(ex.Element, ex.Detail);
            }
            catch (FormatterException ex)
            {
                // Defensive net: a FormatterException from outside the wrapped flush/read/write
                // paths (e.g. building the section list) still fails only this POU, never the batch.
                return new FileFailure(pou.Name, ex.Message);
            }
        }

        private static IReadOnlyList<FileFailure> Compact(FileFailure?[] failures)
        {
            var list = new List<FileFailure>();
            foreach (var failure in failures)
            {
                if (failure != null) list.Add(failure);
            }
            return list;
        }

        // Flush, then read every section. The POU's requests join the batch only once all of its
        // sections were read, so a POU that fails here sends nothing to tctidy.
        private static CapturedPou Capture(IOpenPou pou, List<SectionRequest> requests)
        {
            Flush(pou);

            IReadOnlyList<IStSection> sections;
            try { sections = pou.Sections; }
            catch (FormatterException ex)
            {
                throw new ElementFailure(pou.Name, ex.Message);
            }

            var originals = new string[sections.Count];
            for (var i = 0; i < sections.Count; i++) originals[i] = Read(sections[i]);

            var captured = new CapturedPou(pou, sections, originals, requests.Count);
            for (var i = 0; i < sections.Count; i++)
                requests.Add(new SectionRequest(originals[i], sections[i].Kind, pou.FilePath));
            return captured;
        }

        // Every section was formatted before the edit transaction opens. Once writes begin, every
        // failure runs both explicit per-section restoration and the transaction abort. The two
        // layers cover COM setters that fail after mutating and Visual Studio undo rollback respectively.
        private static bool Apply(CapturedPou captured, IReadOnlyList<SectionResult> results)
        {
            var pou = captured.Pou;
            var edits = new List<SectionEdit>();
            for (var i = 0; i < captured.Sections.Count; i++)
            {
                var section = captured.Sections[i];
                var result = results[captured.FirstRequest + i];
                if (result.Output == null) throw new ElementFailure(section.Name, result.Error!);
                if (result.Output != captured.Originals[i])
                    edits.Add(new SectionEdit(section, captured.Originals[i], result.Output));
            }

            if (edits.Count == 0) return false;

            // The IDE was free while tctidy ran, so the POU may have been edited since it was read.
            // An unsaved edit shows in a read only once flushed, hence the second flush.
            Flush(pou);
            for (var i = 0; i < captured.Sections.Count; i++)
            {
                var section = captured.Sections[i];
                if (Read(section) != captured.Originals[i])
                    throw new ElementFailure(section.Name, "edited while it was being formatted");
            }

            try { pou.BeginEdit(); }
            catch (FormatterException ex)
            {
                throw new ElementFailure(pou.Name, "could not begin edit transaction: " + ex.Message);
            }

            var attempted = new List<SectionEdit>();
            var saveAttempted = false;
            try
            {
                foreach (var edit in edits)
                {
                    // Add before Write: a COM setter may mutate the property and then throw.
                    attempted.Add(edit);
                    try { edit.Section.Write(edit.Formatted); }
                    catch (FormatterException ex)
                    {
                        throw SectionFailure(edit.Section, "write", ex);
                    }
                }

                saveAttempted = true;
                try { pou.Save(); }
                catch (FormatterException ex)
                {
                    throw new ElementFailure(pou.Name, "could not save formatted POU: " + ex.Message);
                }

                try { pou.CommitEdit(); }
                catch (FormatterException ex)
                {
                    throw new ElementFailure(pou.Name, "could not commit edit transaction: " + ex.Message);
                }
                return true;
            }
            catch (ElementFailure failure)
            {
                throw RollBack(pou, attempted, saveAttempted, failure);
            }
        }

        private static ElementFailure RollBack(
            IOpenPou pou,
            IReadOnlyList<SectionEdit> attempted,
            bool saveAttempted,
            ElementFailure primary)
        {
            var recoveryFailures = new List<string>();
            for (var i = attempted.Count - 1; i >= 0; i--)
            {
                var edit = attempted[i];
                try { edit.Section.Restore(edit.Original); }
                catch (FormatterException ex)
                {
                    recoveryFailures.Add(
                        $"rollback failed for {SectionDescription(edit.Section)}: {ex.Message}");
                }
            }

            try { pou.AbortEdit(); }
            catch (FormatterException ex)
            {
                recoveryFailures.Add("could not abort edit transaction: " + ex.Message);
            }

            // Save may have failed after partially persisting. Re-saving the restored/aborted document
            // is the best available way to put the original text back on disk; any failure is reported.
            if (saveAttempted)
            {
                try { pou.Save(); }
                catch (FormatterException ex)
                {
                    recoveryFailures.Add("could not persist rollback: " + ex.Message);
                }
            }

            if (recoveryFailures.Count == 0) return primary;
            return new ElementFailure(
                primary.Element,
                primary.Detail + "; " + string.Join("; ", recoveryFailures));
        }

        private static void Flush(IOpenPou pou)
        {
            try { pou.Flush(); }
            catch (FormatterException ex)
            {
                throw new ElementFailure(pou.Name, ex.Message);
            }
        }

        private static string Read(IStSection section)
        {
            try { return section.Read(); }
            catch (FormatterException ex)
            {
                throw SectionFailure(section, "read", ex);
            }
        }

        private static ElementFailure SectionFailure(
            IStSection section, string operation, FormatterException error) =>
            new ElementFailure(
                section.Name,
                $"could not {operation} {SectionDescription(section)}: {error.Message}");

        private static string SectionDescription(IStSection section)
        {
            switch (section.Kind)
            {
                case StSection.Declaration:
                    return "declaration (DeclarationText)";
                case StSection.Implementation:
                    return "implementation (ImplementationText)";
                default:
                    return "section (text property)";
            }
        }

        // One POU as read on the caller's thread: its sections, their text, and where its requests
        // (one per section, in order) start in the batch.
        private sealed class CapturedPou
        {
            public CapturedPou(
                IOpenPou pou, IReadOnlyList<IStSection> sections, string[] originals, int firstRequest)
            {
                Pou = pou;
                Sections = sections;
                Originals = originals;
                FirstRequest = firstRequest;
            }

            public IOpenPou Pou { get; }
            public IReadOnlyList<IStSection> Sections { get; }
            public string[] Originals { get; }
            public int FirstRequest { get; }
        }

        private sealed class SectionEdit
        {
            public SectionEdit(IStSection section, string original, string formatted)
            {
                Section = section;
                Original = original;
                Formatted = formatted;
            }

            public IStSection Section { get; }
            public string Original { get; }
            public string Formatted { get; }
        }

        // Carries the failing element's name apart from tctidy's diagnostic, so Guard() can build a
        // FileFailure without splitting a pre-joined string. Private: it never escapes this orchestrator.
        private sealed class ElementFailure : Exception
        {
            public ElementFailure(string element, string detail)
            {
                Element = element;
                Detail = detail;
            }

            public string Element { get; }
            public string Detail { get; }
        }
    }
}
