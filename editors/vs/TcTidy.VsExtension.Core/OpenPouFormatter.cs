using System;
using System.Collections.Generic;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// Formats POUs that are open in the editor. For each POU it flushes the dirty buffer first so it
    /// reads what is on screen (because it always reads the on-screen text, the data-loss path where
    /// unrelated unsaved edits were overwritten is closed by construction), then reformats each ST
    /// section with tctidy, then applies all writes inside a per-POU transaction. A failed write, save,
    /// or commit restores every attempted section and aborts the transaction. A
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

        public BatchOutcome Format(IReadOnlyList<IOpenPou> pous)
        {
            if (pous == null) throw new ArgumentNullException(nameof(pous));

            var (changed, unchanged) = (0, 0);
            var failures = new List<FileFailure>();

            foreach (var pou in pous)
            {
                try
                {
                    if (FormatPou(pou)) changed++;
                    else unchanged++;
                }
                catch (ElementFailure ex)
                {
                    // Source is the offending element's name (the POU, or the specific method/action);
                    // the diagnostic stays separate so the report can surface format and Automation
                    // Interface failures per element, rather than naming the parent POU only.
                    failures.Add(new FileFailure(ex.Element, ex.Detail));
                }
                catch (FormatterException ex)
                {
                    // Defensive net: a FormatterException from outside the wrapped flush/format/write
                    // paths (e.g. building the section list) still fails only this POU, never the batch.
                    failures.Add(new FileFailure(pou.Name, ex.Message));
                }
            }

            return new BatchOutcome(changed, unchanged, failures);
        }

        // Flush, read, and format every section before opening the edit transaction. Once writes begin,
        // every failure runs both explicit per-section restoration and the transaction abort. The two
        // layers cover COM setters that fail after mutating and Visual Studio undo rollback respectively.
        private bool FormatPou(IOpenPou pou)
        {
            try { pou.Flush(); }
            catch (FormatterException ex)
            {
                throw new ElementFailure(pou.Name, ex.Message);
            }

            IReadOnlyList<IStSection> sections;
            try { sections = pou.Sections; }
            catch (FormatterException ex)
            {
                throw new ElementFailure(pou.Name, ex.Message);
            }

            var edits = new List<SectionEdit>();
            for (var i = 0; i < sections.Count; i++)
            {
                var section = sections[i];
                string original;
                try { original = section.Read(); }
                catch (FormatterException ex)
                {
                    throw SectionFailure(section, "read", ex);
                }

                try
                {
                    var result = _runner.FormatText(original, section.Kind, pou.FilePath);
                    if (result != original) edits.Add(new SectionEdit(section, original, result));
                }
                catch (FormatterException ex)
                {
                    throw new ElementFailure(section.Name, ex.Message);
                }
            }

            if (edits.Count == 0) return false;

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

        // Carries the failing element's name apart from tctidy's diagnostic, so Format() can build a
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
