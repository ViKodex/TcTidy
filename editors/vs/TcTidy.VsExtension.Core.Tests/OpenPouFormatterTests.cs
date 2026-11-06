using System.Collections.Generic;
using System.Text.RegularExpressions;
using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    /// <summary>
    /// Exercises <see cref="OpenPouFormatter"/> with hand-written fakes (no IDE, no TwinCAT). The fake
    /// POU models the stale read that the flush prevents: a section reads its last-saved (committed)
    /// text until the editor buffer is flushed, after which it reads the on-screen (dirty) text.
    /// </summary>
    public sealed class OpenPouFormatterTests
    {
        [Fact]
        public void Reports_unchanged_and_does_not_save_when_already_formatted()
        {
            var impl = new FakeStSection(StSection.Implementation, committed: "x := 1;", dirty: "x := 1;");
            var pou = new FakeOpenPou("FB_X", impl);

            var outcome = Format(pou);

            Assert.Equal(0, outcome.Changed);
            Assert.Equal(1, outcome.Unchanged);
            Assert.Empty(outcome.Failures);
            Assert.Null(impl.Written);
            Assert.Equal(0, pou.SaveCount);
        }

        [Fact]
        public void Flushes_before_reading_so_the_on_screen_text_is_what_gets_formatted()
        {
            // Flush-before-read regression: the committed (last-saved) text differs from the dirty editor buffer.
            var impl = new FakeStSection(StSection.Implementation, committed: "x := 1;", dirty: "y:=2;");
            var pou = new FakeOpenPou("FB_X", impl);

            var outcome = Format(pou);

            Assert.True(pou.Flushed);
            Assert.Equal("y := 2;", impl.Written); // formatted the DIRTY text, not the committed "x := 1;"
            Assert.Equal(1, outcome.Changed);
        }

        [Fact]
        public void Preserves_unrelated_unsaved_edits_when_the_saved_text_is_unformatted()
        {
            // The data-loss scenario: the saved text is unformatted AND the editor holds an unrelated
            // unsaved edit. The dirty text must be formatted, never the committed text.
            var impl = new FakeStSection(
                StSection.Implementation, committed: "a:=1;", dirty: "a:=1;\r\nb:=2;");
            var pou = new FakeOpenPou("FB_X", impl);

            Format(pou);

            // The unrelated edit (b) survived and was formatted, not overwritten by formatting "a:=1;".
            Assert.Equal("a := 1;\r\nb := 2;", impl.Written);
        }

        [Fact]
        public void A_parse_error_in_the_dirty_buffer_is_recorded_as_a_failure_and_nothing_is_written()
        {
            var impl = new FakeStSection(StSection.Implementation, committed: "x := 1;", dirty: "café:=1;");
            var pou = new FakeOpenPou("FB_X", impl);

            var outcome = Format(pou);

            Assert.Equal(0, outcome.Changed);
            Assert.Single(outcome.Failures);
            Assert.Equal("FB_X", outcome.Failures[0].Source);
            Assert.Equal("1:1: parse error", outcome.Failures[0].Message); // diagnostic kept apart from the name
            Assert.Null(impl.Written);       // left untouched (format-all-before-write-any)
            Assert.Equal(0, pou.SaveCount);  // never saved
        }

        [Fact]
        public void A_failing_child_section_names_the_child_not_just_the_pou()
        {
            // A parse error inside a method must be reported as "FB_X.Init", not just "FB_X".
            var body = new FakeStSection(StSection.Implementation, committed: "x := 1;", dirty: "x := 1;");
            var method = new FakeStSection(
                StSection.Implementation, committed: "café:=1;", dirty: "café:=1;", name: "FB_X.Init");
            var pou = new FakeOpenPou("FB_X", body, method);

            var outcome = Format(pou);

            Assert.Single(outcome.Failures);
            Assert.Equal("FB_X.Init", outcome.Failures[0].Source);       // names the child, not just the POU
            Assert.StartsWith("FB_X.Init:", outcome.Failures[0].ToString()); // and ToString() keeps the joined form
        }

        [Fact]
        public void A_failed_flush_is_reported_and_the_stale_model_is_never_read_or_formatted()
        {
            // If the flush cannot commit the dirty buffer, the stale model must NOT be formatted, that
            // would persist last-saved text over the user's on-screen edits.
            var impl = new FakeStSection(StSection.Implementation, committed: "x := 1;", dirty: "y:=2;");
            var pou = new FakeOpenPou("FB_X", impl)
            {
                FlushError = new FormatterException(-1, "could not save before formatting"),
            };

            var outcome = Format(pou);

            Assert.Single(outcome.Failures);
            Assert.Equal("FB_X", outcome.Failures[0].Source);
            Assert.Equal(0, impl.ReadCount); // never read -> never formatted the stale model
            Assert.Null(impl.Written);
            Assert.Equal(0, pou.SaveCount);
        }

        [Fact]
        public void Formats_each_listed_section_with_its_own_grammar()
        {
            // A POU listing both a declaration and an implementation. Absent sections are simply never
            // listed (decl-only / impl-only), so there is no null to skip.
            var decl = new FakeStSection(
                StSection.Declaration, committed: "VAR\r\na : INT;\r\nEND_VAR", dirty: "VAR\r\na : INT;\r\nEND_VAR");
            var impl = new FakeStSection(StSection.Implementation, committed: "x := 1;", dirty: "x := 1;");
            var pou = new FakeOpenPou("FB_X", decl, impl);
            var runner = new FakeRunner();

            new OpenPouFormatter(runner).Format(new IOpenPou[] { pou });

            Assert.Contains((decl.Read(), StSection.Declaration), runner.Calls);
            Assert.Contains((impl.Read(), StSection.Implementation), runner.Calls);
        }

        [Fact]
        public void Formats_every_section_for_the_POU_file_so_its_config_applies()
        {
            var decl = new FakeStSection(StSection.Declaration, committed: "VAR\r\nEND_VAR", dirty: "VAR\r\nEND_VAR");
            var impl = new FakeStSection(StSection.Implementation, committed: "x:=1;", dirty: "x:=1;");
            var pou = new FakeOpenPou("FB_X", decl, impl) { FilePath = @"C:\Plc\POUs\FB_X.TcPOU" };
            var runner = new FakeRunner();

            new OpenPouFormatter(runner).Format(new IOpenPou[] { pou });

            Assert.Equal(new[] { pou.FilePath, pou.FilePath }, runner.SourcePaths);
        }

        [Fact]
        public void Saves_once_when_any_section_changed()
        {
            var decl = new FakeStSection( // already formatted -> unchanged
                StSection.Declaration, committed: "VAR\r\na : INT;\r\nEND_VAR", dirty: "VAR\r\na : INT;\r\nEND_VAR");
            var impl = new FakeStSection(StSection.Implementation, committed: "x:=1;", dirty: "x:=1;"); // changes

            var pou = new FakeOpenPou("FB_X", decl, impl);

            var outcome = Format(pou);

            Assert.Equal(1, outcome.Changed);
            Assert.Equal(1, pou.SaveCount);
            Assert.Equal(1, pou.BeginEditCount);
            Assert.Equal(1, pou.CommitEditCount);
            Assert.Equal(0, pou.AbortEditCount);
        }

        [Theory]
        [InlineData(0, "declaration", "DeclarationText")]
        [InlineData(1, "implementation", "ImplementationText")]
        public void Read_failure_identifies_the_section_property_and_underlying_error(
            int failingIndex, string sectionName, string propertyName)
        {
            var sections = ChangedSections();
            sections[failingIndex].ReadError = new FormatterException(-1, "COM read boom");
            var pou = new FakeOpenPou("FB_X", sections);

            var outcome = Format(pou);

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
        public void Write_failure_rolls_back_every_attempted_section(
            int failingIndex, string sectionName, string propertyName)
        {
            var sections = ChangedSections();
            sections[failingIndex].WriteError = new FormatterException(-1, "COM write boom");
            var pou = new FakeOpenPou("FB_X", sections);

            var outcome = Format(pou);

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
        public void Aborted_transaction_recovers_when_section_rollback_fails(
            int failingIndex, string sectionName, string propertyName)
        {
            var sections = ChangedSections();
            sections[1].WriteError = new FormatterException(-1, "COM write boom");
            sections[failingIndex].RestoreError = new FormatterException(-1, "COM rollback boom");
            var pou = new FakeOpenPou("FB_X", sections);

            var outcome = Format(pou);

            var failure = Assert.Single(outcome.Failures);
            Assert.Contains(
                $"rollback failed for {sectionName} ({propertyName}): COM rollback boom",
                failure.Message);
            Assert.Equal(1, pou.AbortEditCount);
            Assert.All(sections, section => Assert.Equal(1, section.RestoreCount));
            Assert.All(sections, section => Assert.Equal(section.Original, section.Current));
        }

        [Fact]
        public void Save_failure_is_reported_and_all_sections_are_rolled_back()
        {
            var sections = ChangedSections();
            var pou = new FakeOpenPou("FB_X", sections)
            {
                SaveError = new FormatterException(-1, "disk full"),
            };

            var outcome = Format(pou);

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
        public void A_failing_pou_does_not_abort_an_unrelated_pou()
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

            var outcome = Format(failed, healthy);

            Assert.Equal(1, outcome.Changed);
            Assert.Equal(0, outcome.Unchanged);
            Assert.Single(outcome.Failures);
            Assert.Equal("FB_Failed", outcome.Failures[0].Source);
            Assert.Equal(failedSection.Original, failedSection.Current);
            Assert.Equal("y := 2;", healthySection.Current);
            Assert.Equal(1, healthy.SaveCount);
            Assert.Equal(1, healthy.CommitEditCount);
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

        private static BatchOutcome Format(params FakeOpenPou[] pous) =>
            new OpenPouFormatter(new FakeRunner()).Format(pous);

        // ----- fakes -----

        // A trivial deterministic "formatter": normalizes whitespace around ":=" to " := " (idempotent,
        // so already-formatted text is returned unchanged) and refuses a Unicode identifier the way
        // tctidy refuses a parse error.
        private sealed class FakeRunner : ITctidyRunner
        {
            public List<(string Source, StSection Kind)> Calls { get; } = new();
            public List<string?> SourcePaths { get; } = new();

            public string FormatText(string source, StSection section, string? sourcePath = null)
            {
                Calls.Add((source, section));
                SourcePaths.Add(sourcePath);
                if (source.Contains("café")) throw new FormatterException(3, "1:1: parse error");
                return Regex.Replace(source, @"\s*:=\s*", " := ");
            }

            public string FormatXml(string xml) => xml;
            public FormatResult FormatFile(string path) => throw new System.NotSupportedException();
            public BatchOutcome FormatFiles(IReadOnlyList<string> paths) => throw new System.NotSupportedException();
        }

        // A section whose Read() returns the committed (last-saved) text until its owning POU is
        // flushed, after which it returns the dirty (on-screen) text, as the Automation Interface does.
        private sealed class FakeStSection : IStSection
        {
            private readonly string _committed;
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
                ReadCount++;
                if (ReadError != null) throw ReadError;
                return Owner!.Flushed ? Current : _committed;
            }

            public void Write(string text)
            {
                Written = text;
                Current = text;
                if (WriteError != null) throw WriteError;
            }

            public void Restore(string text)
            {
                RestoreCount++;
                if (RestoreError != null) throw RestoreError;
                Current = text;
            }

            internal void RestoreTransactionSnapshot(string text) => Current = text;
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
            public bool Flushed { get; private set; }
            public int SaveCount { get; private set; }
            public int BeginEditCount { get; private set; }
            public int CommitEditCount { get; private set; }
            public int AbortEditCount { get; private set; }
            public FormatterException? SaveError { get; set; }
            private string[]? _snapshot;

            /// <summary>When set, <see cref="Flush"/> throws it (a flush that cannot commit).</summary>
            public FormatterException? FlushError { get; set; }

            public void Flush()
            {
                if (FlushError != null) throw FlushError;
                Flushed = true;
            }

            public void BeginEdit()
            {
                BeginEditCount++;
                var sections = (FakeStSection[])Sections;
                _snapshot = new string[sections.Length];
                for (var i = 0; i < sections.Length; i++) _snapshot[i] = sections[i].Current;
            }

            public void CommitEdit()
            {
                CommitEditCount++;
                _snapshot = null;
            }

            public void AbortEdit()
            {
                AbortEditCount++;
                var sections = (FakeStSection[])Sections;
                for (var i = 0; i < sections.Length; i++)
                    sections[i].RestoreTransactionSnapshot(_snapshot![i]);
                _snapshot = null;
            }

            public void Save()
            {
                SaveCount++;
                if (SaveError != null) throw SaveError;
            }
        }
    }
}
