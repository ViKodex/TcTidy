using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.RegularExpressions;
using System.Threading;
using System.Threading.Tasks;
using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    /// <summary>
    /// Exercises <see cref="OpenPouFormatter"/> with hand-written fakes (no IDE, no TwinCAT). The fake
    /// POU models the stale read that the flush prevents: a section reads its last-saved (committed)
    /// text until the editor buffer is flushed, after which it reads the on-screen (dirty) text. The
    /// fake runner stands for the one tctidy run, where a test can act while the IDE is free.
    /// </summary>
    public sealed class OpenPouFormatterTests
    {
        [Fact]
        public async Task Reports_unchanged_and_does_not_save_when_already_formatted()
        {
            var impl = new FakeStSection(StSection.Implementation, committed: "x := 1;", dirty: "x := 1;");
            var pou = new FakeOpenPou("FB_X", impl);

            var outcome = await Format(pou);

            Assert.Equal(0, outcome.Changed);
            Assert.Equal(1, outcome.Unchanged);
            Assert.Empty(outcome.Failures);
            Assert.Null(impl.Written);
            Assert.Equal(0, pou.SaveCount);
            Assert.Equal(1, pou.FlushCount); // nothing to write, so nothing to check again
        }

        [Fact]
        public async Task Flushes_before_reading_so_the_on_screen_text_is_what_gets_formatted()
        {
            // Flush-before-read regression: the committed (last-saved) text differs from the dirty editor buffer.
            var impl = new FakeStSection(StSection.Implementation, committed: "x := 1;", dirty: "y:=2;");
            var pou = new FakeOpenPou("FB_X", impl);

            var outcome = await Format(pou);

            Assert.True(pou.FlushCount > 0);
            Assert.Equal("y := 2;", impl.Written); // formatted the DIRTY text, not the committed "x := 1;"
            Assert.Equal(1, outcome.Changed);
        }

        [Fact]
        public async Task Preserves_unrelated_unsaved_edits_when_the_saved_text_is_unformatted()
        {
            // The data-loss scenario: the saved text is unformatted AND the editor holds an unrelated
            // unsaved edit. The dirty text must be formatted, never the committed text.
            var impl = new FakeStSection(
                StSection.Implementation, committed: "a:=1;", dirty: "a:=1;\r\nb:=2;");
            var pou = new FakeOpenPou("FB_X", impl);

            await Format(pou);

            // The unrelated edit (b) survived and was formatted, not overwritten by formatting "a:=1;".
            Assert.Equal("a := 1;\r\nb := 2;", impl.Written);
        }

        [Fact]
        public async Task A_parse_error_in_the_dirty_buffer_is_recorded_as_a_failure_and_nothing_is_written()
        {
            var impl = new FakeStSection(StSection.Implementation, committed: "x := 1;", dirty: "café:=1;");
            var pou = new FakeOpenPou("FB_X", impl);

            var outcome = await Format(pou);

            Assert.Equal(0, outcome.Changed);
            Assert.Single(outcome.Failures);
            Assert.Equal("FB_X", outcome.Failures[0].Source);
            Assert.Equal("1:1: parse error", outcome.Failures[0].Message); // diagnostic kept apart from the name
            Assert.Null(impl.Written);       // left untouched (format-all-before-write-any)
            Assert.Equal(0, pou.SaveCount);  // never saved
        }

        [Fact]
        public async Task A_failing_child_section_names_the_child_not_just_the_pou()
        {
            // A parse error inside a method must be reported as "FB_X.Init", not just "FB_X".
            var body = new FakeStSection(StSection.Implementation, committed: "x := 1;", dirty: "x := 1;");
            var method = new FakeStSection(
                StSection.Implementation, committed: "café:=1;", dirty: "café:=1;", name: "FB_X.Init");
            var pou = new FakeOpenPou("FB_X", body, method);

            var outcome = await Format(pou);

            Assert.Single(outcome.Failures);
            Assert.Equal("FB_X.Init", outcome.Failures[0].Source);       // names the child, not just the POU
            Assert.StartsWith("FB_X.Init:", outcome.Failures[0].ToString()); // and ToString() keeps the joined form
        }

        [Fact]
        public async Task A_failed_flush_is_reported_and_the_stale_model_is_never_read_or_formatted()
        {
            // If the flush cannot commit the dirty buffer, the stale model must NOT be formatted, that
            // would persist last-saved text over the user's on-screen edits.
            var impl = new FakeStSection(StSection.Implementation, committed: "x := 1;", dirty: "y:=2;");
            var pou = new FakeOpenPou("FB_X", impl)
            {
                FlushError = new FormatterException(-1, "could not save before formatting"),
            };

            var outcome = await Format(pou);

            Assert.Single(outcome.Failures);
            Assert.Equal("FB_X", outcome.Failures[0].Source);
            Assert.Equal(0, impl.ReadCount); // never read -> never formatted the stale model
            Assert.Null(impl.Written);
            Assert.Equal(0, pou.SaveCount);
        }

        [Fact]
        public async Task Formats_each_listed_section_with_its_own_grammar()
        {
            // A POU listing both a declaration and an implementation. Absent sections are simply never
            // listed (decl-only / impl-only), so there is no null to skip.
            var decl = new FakeStSection(
                StSection.Declaration, committed: "VAR\r\na : INT;\r\nEND_VAR", dirty: "VAR\r\na : INT;\r\nEND_VAR");
            var impl = new FakeStSection(StSection.Implementation, committed: "x := 1;", dirty: "x := 1;");
            var pou = new FakeOpenPou("FB_X", decl, impl);
            var runner = new FakeRunner();

            await Format(runner, pou);

            Assert.Equal(
                new[] { (decl.Current, StSection.Declaration), (impl.Current, StSection.Implementation) },
                runner.Requests.Select(r => (r.Source, r.Kind)));
        }

        [Fact]
        public async Task Formats_every_section_for_the_POU_file_so_its_config_applies()
        {
            var decl = new FakeStSection(StSection.Declaration, committed: "VAR\r\nEND_VAR", dirty: "VAR\r\nEND_VAR");
            var impl = new FakeStSection(StSection.Implementation, committed: "x:=1;", dirty: "x:=1;");
            var pou = new FakeOpenPou("FB_X", decl, impl) { FilePath = @"C:\Plc\POUs\FB_X.TcPOU" };
            var runner = new FakeRunner();

            await Format(runner, pou);

            Assert.Equal(new[] { pou.FilePath, pou.FilePath }, runner.Requests.Select(r => r.SourcePath));
        }

        [Fact]
        public async Task Saves_once_when_any_section_changed()
        {
            var decl = new FakeStSection( // already formatted -> unchanged
                StSection.Declaration, committed: "VAR\r\na : INT;\r\nEND_VAR", dirty: "VAR\r\na : INT;\r\nEND_VAR");
            var impl = new FakeStSection(StSection.Implementation, committed: "x:=1;", dirty: "x:=1;"); // changes

            var pou = new FakeOpenPou("FB_X", decl, impl);

            var outcome = await Format(pou);

            Assert.Equal(1, outcome.Changed);
            Assert.Equal(1, pou.SaveCount);
            Assert.Equal(1, pou.BeginEditCount);
            Assert.Equal(1, pou.CommitEditCount);
            Assert.Equal(0, pou.AbortEditCount);
        }

        [Theory]
        [InlineData(0, "declaration", "DeclarationText")]
        [InlineData(1, "implementation", "ImplementationText")]
        public async Task Read_failure_identifies_the_section_property_and_underlying_error(
            int failingIndex, string sectionName, string propertyName)
        {
            var sections = ChangedSections();
            sections[failingIndex].ReadError = new FormatterException(-1, "COM read boom");
            var pou = new FakeOpenPou("FB_X", sections);

            var outcome = await Format(pou);

            var failure = Assert.Single(outcome.Failures);
            Assert.Equal("FB_X", failure.Source);
            Assert.Equal(
                $"could not read {sectionName} ({propertyName}): COM read boom",
                failure.Message);
            Assert.Equal(0, pou.BeginEditCount);
            Assert.All(sections, section => Assert.Equal(section.Original, section.Current));
        }

        [Theory]
        [InlineData(0, "declaration", "DeclarationText")]
        [InlineData(1, "implementation", "ImplementationText")]
        public async Task Write_failure_rolls_back_every_attempted_section(
            int failingIndex, string sectionName, string propertyName)
        {
            var sections = ChangedSections();
            sections[failingIndex].WriteError = new FormatterException(-1, "COM write boom");
            var pou = new FakeOpenPou("FB_X", sections);

            var outcome = await Format(pou);

            var failure = Assert.Single(outcome.Failures);
            Assert.Equal("FB_X", failure.Source);
            Assert.Equal(
                $"could not write {sectionName} ({propertyName}): COM write boom",
                failure.Message);
            Assert.Equal(0, outcome.Changed);
            Assert.Equal(0, pou.SaveCount);
            Assert.Equal(1, pou.AbortEditCount);
            Assert.Equal(0, pou.CommitEditCount);
            for (var i = 0; i < sections.Length; i++)
                Assert.Equal(i <= failingIndex ? 1 : 0, sections[i].RestoreCount);
            Assert.All(sections, section => Assert.Equal(section.Original, section.Current));
        }

        [Theory]
        [InlineData(0, "declaration", "DeclarationText")]
        [InlineData(1, "implementation", "ImplementationText")]
        public async Task Aborted_transaction_recovers_when_section_rollback_fails(
            int failingIndex, string sectionName, string propertyName)
        {
            var sections = ChangedSections();
            sections[1].WriteError = new FormatterException(-1, "COM write boom");
            sections[failingIndex].RestoreError = new FormatterException(-1, "COM rollback boom");
            var pou = new FakeOpenPou("FB_X", sections);

            var outcome = await Format(pou);

            var failure = Assert.Single(outcome.Failures);
            Assert.Contains(
                $"rollback failed for {sectionName} ({propertyName}): COM rollback boom",
                failure.Message);
            Assert.Equal(1, pou.AbortEditCount);
            Assert.All(sections, section => Assert.Equal(1, section.RestoreCount));
            Assert.All(sections, section => Assert.Equal(section.Original, section.Current));
        }

        [Fact]
        public async Task Save_failure_is_reported_and_all_sections_are_rolled_back()
        {
            var sections = ChangedSections();
            var pou = new FakeOpenPou("FB_X", sections)
            {
                SaveError = new FormatterException(-1, "disk full"),
            };

            var outcome = await Format(pou);

            var failure = Assert.Single(outcome.Failures);
            Assert.Equal("FB_X", failure.Source);
            Assert.Equal(
                "could not save formatted POU: disk full; could not persist rollback: disk full",
                failure.Message);
            Assert.Equal(0, outcome.Changed);
            Assert.Equal(0, outcome.Unchanged);
            Assert.Equal(2, pou.SaveCount);
            Assert.Equal(1, pou.AbortEditCount);
            Assert.Equal(0, pou.CommitEditCount);
            Assert.All(sections, section => Assert.Equal(1, section.RestoreCount));
            Assert.All(sections, section => Assert.Equal(section.Original, section.Current));
        }

        [Fact]
        public async Task A_failing_pou_does_not_abort_an_unrelated_pou()
        {
            var failedSection = new FakeStSection(
                StSection.Implementation, committed: "x:=1;", dirty: "x:=1;")
            {
                WriteError = new FormatterException(-1, "COM write boom"),
            };
            var failed = new FakeOpenPou("FB_Failed", failedSection);
            var healthySection = new FakeStSection(
                StSection.Implementation, committed: "y:=2;", dirty: "y:=2;");
            var healthy = new FakeOpenPou("FB_Healthy", healthySection);

            var outcome = await Format(failed, healthy);

            Assert.Equal(1, outcome.Changed);
            Assert.Equal(0, outcome.Unchanged);
            Assert.Single(outcome.Failures);
            Assert.Equal("FB_Failed", outcome.Failures[0].Source);
            Assert.Equal(failedSection.Original, failedSection.Current);
            Assert.Equal("y := 2;", healthySection.Current);
            Assert.Equal(1, healthy.SaveCount);
            Assert.Equal(1, healthy.CommitEditCount);
        }

        [Fact]
        public async Task Formats_the_sections_of_every_pou_in_one_tctidy_run()
        {
            var first = new FakeOpenPou("FB_A", ChangedSections());
            var second = new FakeOpenPou("FB_B", ChangedSections());
            var runner = new FakeRunner();

            var outcome = await Format(runner, first, second);

            Assert.Equal(4, Assert.Single(runner.Runs).Count);
            Assert.Equal(2, outcome.Changed);
        }

        [Fact]
        public async Task Touches_the_pous_on_the_calling_thread_and_runs_tctidy_off_it()
        {
            using var ui = new UiThread();
            var pou = new FakeOpenPou("FB_X", ChangedSections());
            var runner = new FakeRunner();

            var outcome = await ui.Run(() => Format(runner, pou));

            // The flush, the reads, the check before writing, the transaction, the writes, the save.
            Assert.Equal(1, outcome.Changed);
            Assert.Equal(new[] { ui.Id }, pou.Threads);
            Assert.NotEqual(ui.Id, runner.ThreadId);
        }

        [Fact]
        public async Task Leaves_the_calling_thread_free_while_tctidy_runs()
        {
            using var ui = new UiThread();
            var pou = new FakeOpenPou("FB_X", ChangedSections());
            var handled = false;
            var runner = new FakeRunner
            {
                // tctidy does not return until the calling thread has handled another message.
                WhileRunning = () =>
                {
                    using var message = new ManualResetEventSlim();
                    ui.Post(message.Set);
                    handled = message.Wait(TimeSpan.FromSeconds(30));
                },
            };

            var outcome = await ui.Run(() => Format(runner, pou));

            Assert.True(handled);
            Assert.Equal(1, outcome.Changed);
        }

        [Fact]
        public async Task A_pou_edited_while_tctidy_ran_is_reported_and_left_untouched()
        {
            // The body needs formatting; the method does not, but the user types in it meanwhile.
            // That edit is unsaved, so only the flush before the check brings it to the reads.
            var body = new FakeStSection(StSection.Implementation, committed: "x:=1;", dirty: "x:=1;");
            var method = new FakeStSection(
                StSection.Implementation, committed: "y := 2;", dirty: "y := 2;", name: "FB_X.Init");
            var edited = new FakeOpenPou("FB_X", body, method);
            var otherSection = new FakeStSection(StSection.Implementation, committed: "z:=3;", dirty: "z:=3;");
            var other = new FakeOpenPou("FB_Y", otherSection);
            var runner = new FakeRunner { WhileRunning = () => method.TypeInEditor("y := 2;\r\nw := 4;") };

            var outcome = await Format(runner, edited, other);

            var failure = Assert.Single(outcome.Failures);
            Assert.Equal("FB_X.Init", failure.Source);
            Assert.Equal("edited while it was being formatted", failure.Message);
            Assert.Null(body.Written);
            Assert.Equal(0, edited.BeginEditCount);
            Assert.Equal(0, edited.SaveCount);
            Assert.Equal("y := 2;\r\nw := 4;", method.Current); // the edit is kept as typed
            Assert.Equal(1, outcome.Changed);
            Assert.Equal("z := 3;", otherSection.Current);
        }

        [Fact]
        public async Task A_tctidy_run_that_fails_fails_every_pou_it_was_to_format()
        {
            var unreadable = ChangedSections();
            unreadable[0].ReadError = new FormatterException(-1, "COM read boom");
            var pous = new[]
            {
                new FakeOpenPou("FB_A", ChangedSections()),
                new FakeOpenPou("FB_B", unreadable),
                new FakeOpenPou("FB_C", ChangedSections()),
            };
            var runner = new FakeRunner { Error = new FormatterException(-1, "tctidy timed out after 60s") };

            var outcome = await Format(runner, pous);

            Assert.Equal(0, outcome.Changed);
            Assert.Equal(0, outcome.Unchanged);
            Assert.Equal(
                new[]
                {
                    "FB_A: tctidy timed out after 60s",
                    "FB_B: could not read declaration (DeclarationText): COM read boom",
                    "FB_C: tctidy timed out after 60s",
                },
                outcome.Failures.Select(f => f.ToString()));
            Assert.All(pous, pou => Assert.Equal(0, pou.BeginEditCount));
            Assert.All(pous, pou => Assert.Equal(0, pou.SaveCount));
        }

        [Fact]
        public async Task A_tctidy_run_without_a_result_per_section_writes_nothing()
        {
            var pous = new[]
            {
                new FakeOpenPou("FB_A", ChangedSections()),
                new FakeOpenPou("FB_B", ChangedSections()),
            };
            var runner = new FakeRunner { MissingResults = 1 };

            var outcome = await Format(runner, pous);

            Assert.Equal(0, outcome.Changed);
            Assert.Equal(new[] { "FB_A", "FB_B" }, outcome.Failures.Select(f => f.Source));
            Assert.All(
                outcome.Failures,
                f => Assert.Equal("tctidy did not return one result per section", f.Message));
            Assert.All(pous, pou => Assert.Equal(0, pou.BeginEditCount));
        }

        private static FakeStSection[] ChangedSections() => new[]
        {
            new FakeStSection(
                StSection.Declaration,
                committed: "VAR\r\na : INT :=1;\r\nEND_VAR",
                dirty: "VAR\r\na : INT :=1;\r\nEND_VAR"),
            new FakeStSection(
                StSection.Implementation,
                committed: "x:=1;",
                dirty: "x:=1;"),
        };

        private static Task<BatchOutcome> Format(params FakeOpenPou[] pous) => Format(new FakeRunner(), pous);

        private static Task<BatchOutcome> Format(FakeRunner runner, params FakeOpenPou[] pous) =>
            new OpenPouFormatter(runner).FormatAsync(pous);

        // ----- fakes -----

        // A trivial deterministic "formatter": normalizes whitespace around ":=" to " := " (idempotent,
        // so already-formatted text is returned unchanged) and refuses a Unicode identifier the way
        // tctidy refuses a parse error.
        private sealed class FakeRunner : ITctidyRunner
        {
            /// <summary>The sections of each run.</summary>
            public List<IReadOnlyList<SectionRequest>> Runs { get; } = new();
            public IEnumerable<SectionRequest> Requests => Runs.SelectMany(run => run);
            public int ThreadId { get; private set; }

            /// <summary>Called during a run, on its thread.</summary>
            public Action? WhileRunning { get; set; }

            /// <summary>When set, a run throws it (tctidy could not start, timed out).</summary>
            public FormatterException? Error { get; set; }

            /// <summary>How many results a run leaves out, from the end.</summary>
            public int MissingResults { get; set; }

            public IReadOnlyList<SectionResult> FormatSections(IReadOnlyList<SectionRequest> sections)
            {
                Runs.Add(sections);
                ThreadId = Environment.CurrentManagedThreadId;
                WhileRunning?.Invoke();
                if (Error != null) throw Error;
                return sections
                    .Select(section => section.Source.Contains("café")
                        ? SectionResult.Refused("1:1: parse error")
                        : SectionResult.Formatted(Regex.Replace(section.Source, @"\s*:=\s*", " := ")))
                    .Take(sections.Count - MissingResults)
                    .ToList();
            }

            public string FormatXml(string xml) => xml;
            public FormatResult FormatFile(string path) => throw new System.NotSupportedException();
            public BatchOutcome FormatFiles(IReadOnlyList<string> paths) => throw new System.NotSupportedException();
        }

        // A section whose Read() returns the committed (last-saved) text until its owning POU is
        // flushed, after which it returns the dirty (on-screen) text, as the Automation Interface does.
        private sealed class FakeStSection : IStSection
        {
            private string _committed;
            private readonly string? _name;
            internal FakeOpenPou? Owner; // wired by the FakeOpenPou ctor

            public FakeStSection(StSection kind, string committed, string dirty, string? name = null)
            {
                Kind = kind;
                _committed = committed;
                Original = dirty;
                Current = dirty;
                _name = name;
            }

            // Defaults to the owning POU's name; a section can override it to model a child (e.g. a method).
            public string Name => _name ?? Owner!.Name;
            public StSection Kind { get; }
            public int ReadCount { get; private set; }
            public string? Written { get; private set; }
            public string Original { get; }
            public string Current { get; private set; }
            public int RestoreCount { get; private set; }
            public FormatterException? ReadError { get; set; }
            public FormatterException? WriteError { get; set; }
            public FormatterException? RestoreError { get; set; }

            public string Read()
            {
                Owner!.Touch();
                ReadCount++;
                if (ReadError != null) throw ReadError;
                return _committed;
            }

            public void Write(string text)
            {
                Owner!.Touch();
                Written = text;
                Set(text);
                if (WriteError != null) throw WriteError;
            }

            public void Restore(string text)
            {
                Owner!.Touch();
                RestoreCount++;
                if (RestoreError != null) throw RestoreError;
                Set(text);
            }

            /// <summary>An edit the user types in the editor: on screen, not yet committed.</summary>
            public void TypeInEditor(string text) => Current = text;

            internal void Flush() => _committed = Current;

            internal void RestoreTransactionSnapshot(string text) => Set(text);

            private void Set(string text)
            {
                Current = text;
                _committed = text;
            }
        }

        private sealed class FakeOpenPou : IOpenPou
        {
            public FakeOpenPou(string name, params FakeStSection[] sections)
            {
                Name = name;
                foreach (var s in sections) s.Owner = this;
                Sections = sections;
            }

            public string Name { get; }
            public string? FilePath { get; set; }
            public IReadOnlyList<IStSection> Sections { get; }
            public int FlushCount { get; private set; }
            public int SaveCount { get; private set; }
            public int BeginEditCount { get; private set; }
            public int CommitEditCount { get; private set; }
            public int AbortEditCount { get; private set; }
            public FormatterException? SaveError { get; set; }
            private string[]? _snapshot;

            /// <summary>When set, <see cref="Flush"/> throws it (a flush that cannot commit).</summary>
            public FormatterException? FlushError { get; set; }

            /// <summary>The threads the POU and its sections were used on.</summary>
            public HashSet<int> Threads { get; } = new();

            internal void Touch() => Threads.Add(Environment.CurrentManagedThreadId);

            public void Flush()
            {
                Touch();
                if (FlushError != null) throw FlushError;
                FlushCount++;
                foreach (var section in (FakeStSection[])Sections) section.Flush();
            }

            public void BeginEdit()
            {
                Touch();
                BeginEditCount++;
                var sections = (FakeStSection[])Sections;
                _snapshot = new string[sections.Length];
                for (var i = 0; i < sections.Length; i++) _snapshot[i] = sections[i].Current;
            }

            public void CommitEdit()
            {
                Touch();
                CommitEditCount++;
                _snapshot = null;
            }

            public void AbortEdit()
            {
                Touch();
                AbortEditCount++;
                var sections = (FakeStSection[])Sections;
                for (var i = 0; i < sections.Length; i++)
                    sections[i].RestoreTransactionSnapshot(_snapshot![i]);
                _snapshot = null;
            }

            public void Save()
            {
                Touch();
                SaveCount++;
                if (SaveError != null) throw SaveError;
            }
        }
    }
}
