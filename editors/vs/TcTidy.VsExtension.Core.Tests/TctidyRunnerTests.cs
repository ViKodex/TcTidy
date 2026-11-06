using System;
using System.IO;
using System.Linq;
using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    /// <summary>
    /// Exercises <see cref="TctidyRunner"/> against the real tctidy binary. Point the
    /// <c>TCTIDY_BIN</c> environment variable at a built binary to run these.
    /// </summary>
    public sealed class TctidyRunnerTests
    {
        private static string TctidyPath()
        {
            var p = Environment.GetEnvironmentVariable("TCTIDY_BIN");
            if (string.IsNullOrEmpty(p) || !File.Exists(p))
            {
                throw new InvalidOperationException(
                    "Set TCTIDY_BIN to a built tctidy binary to run these tests.");
            }
            return p!;
        }

        private static ITctidyRunner Runner() =>
            new TctidyRunner(TctidyPath(), TimeSpan.FromSeconds(10));

        [Fact]
        public void FormatText_formats_an_implementation_assignment()
        {
            var output = Runner().FormatText("x:=1;", StSection.Implementation);
            Assert.Equal("x := 1;", output.TrimEnd('\r', '\n'));
        }

        [Fact]
        public void FormatText_formats_a_declaration()
        {
            var output = Runner().FormatText("VAR\na:INT;\nEND_VAR", StSection.Declaration);
            Assert.Contains("a : INT;", output);
        }

        [Fact]
        public void FormatText_preserves_crlf_line_endings()
        {
            var output = Runner().FormatText(
                "x:=1;\r\nIF a THEN\r\nb:=2;\r\nEND_IF", StSection.Implementation);
            Assert.Contains("\r\n", output);
        }

        [Fact]
        public void FormatText_uses_the_config_of_the_source_file()
        {
            // VS runs tctidy from its own directory; the POU's path is what finds the solution's file.
            var solution = Path.Combine(Path.GetTempPath(), $"tctidy-sln-{Guid.NewGuid():N}");
            var plc = Path.Combine(solution, "Plc", "POUs");
            Directory.CreateDirectory(plc);
            try
            {
                File.WriteAllText(Path.Combine(solution, ".tctidy.toml"), "keyword_case = \"lower\"\n");
                var pou = Path.Combine(plc, "FB_Unsaved.TcPOU"); // need not exist

                var output = Runner().FormatText("IF a THEN\nb:=1;\nEND_IF", StSection.Implementation, pou);

                Assert.StartsWith("if a then", output);
                Assert.StartsWith("IF a THEN", Runner().FormatText("IF a THEN\nb:=1;\nEND_IF", StSection.Implementation));
            }
            finally { Directory.Delete(solution, recursive: true); }
        }

        [Fact]
        public void FormatText_without_a_source_path_uses_the_working_directory_config()
        {
            var solution = Path.Combine(Path.GetTempPath(), $"tctidy-sln-{Guid.NewGuid():N}");
            Directory.CreateDirectory(solution);
            try
            {
                File.WriteAllText(Path.Combine(solution, ".tctidy.toml"), "indent_width = 2\n");
                var runner = new TctidyRunner(TctidyPath(), TimeSpan.FromSeconds(10), solution);

                var output = runner.FormatText("IF a THEN\nb:=1;\nEND_IF", StSection.Implementation);

                Assert.Contains("\n  b := 1;", output);
            }
            finally { Directory.Delete(solution, recursive: true); }
        }

        [Fact]
        public void FormatText_throws_with_exit_code_3_on_parse_error()
        {
            // A Unicode identifier the lexer rejects → tctidy refuses (exit 3).
            var ex = Assert.Throws<FormatterException>(
                () => Runner().FormatText("café : INT;", StSection.Declaration));
            Assert.Equal(3, ex.ExitCode);
        }

        [Fact]
        public void FormatXml_formats_a_twincat_document_and_preserves_the_container()
        {
            var formatted = Runner().FormatXml(MessyDut("X"));

            Assert.Contains("TYPE X : STRUCT", formatted);
            Assert.Contains("a : INT;", formatted);
            // Everything outside the ST is preserved.
            Assert.Contains("Id=\"{00000000-0000-0000-0000-000000000000}\"", formatted);
            Assert.Contains("<TcPlcObject Version=\"1.1.0.1\">", formatted);
            // Idempotent: formatting the formatted document is a no-op.
            Assert.Equal(formatted, Runner().FormatXml(formatted));
        }

        [Fact]
        public void FormatXml_throws_with_exit_code_3_on_a_parse_error()
        {
            // A Unicode identifier the lexer rejects → tctidy refuses (exit 3), document kept.
            var ex = Assert.Throws<FormatterException>(
                () => Runner().FormatXml(MessyDut("Bad").Replace("a:INT;", "café:INT;")));
            Assert.Equal(3, ex.ExitCode);
        }

        [Fact]
        public void FormatFile_formats_a_twincat_file_in_place_and_reports_changed()
        {
            var path = NewTempFile(".TcDUT",
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n" +
                "<TcPlcObject Version=\"1.1.0.1\">\r\n" +
                "  <DUT Name=\"X\" Id=\"{00000000-0000-0000-0000-000000000000}\">\r\n" +
                "    <Declaration><![CDATA[tYpE X : sTrUcT\r\na:INT;\r\neNd_sTrUcT\r\neNd_tYpE]]></Declaration>\r\n" +
                "  </DUT>\r\n" +
                "</TcPlcObject>\r\n");
            try
            {
                var result = Runner().FormatFile(path);

                Assert.True(result.Changed);
                var after = File.ReadAllText(path);
                Assert.Contains("TYPE X : STRUCT", after);
                // The XML container is preserved byte-for-byte.
                Assert.Contains("Id=\"{00000000-0000-0000-0000-000000000000}\"", after);
            }
            finally { File.Delete(path); }
        }

        [Fact]
        public void FormatFile_reports_unchanged_on_an_already_formatted_file()
        {
            var path = NewTempFile(".TcDUT",
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n" +
                "<TcPlcObject Version=\"1.1.0.1\">\r\n" +
                "  <DUT Name=\"X\" Id=\"{00000000-0000-0000-0000-000000000000}\">\r\n" +
                "    <Declaration><![CDATA[tYpE X : sTrUcT\r\na:INT;\r\neNd_sTrUcT\r\neNd_tYpE]]></Declaration>\r\n" +
                "  </DUT>\r\n" +
                "</TcPlcObject>\r\n");
            try
            {
                Runner().FormatFile(path);              // first pass formats it
                var second = Runner().FormatFile(path); // second pass is a no-op
                Assert.False(second.Changed);
            }
            finally { File.Delete(path); }
        }

        [Fact]
        public void FormatFile_throws_FormatterException_when_the_file_is_missing()
        {
            // The "before" read fails before tctidy is even spawned; it must surface as a
            // FormatterException (exit -1) so a batch caller records it instead of aborting.
            var missing = Path.Combine(Path.GetTempPath(), $"tctidy-missing-{Guid.NewGuid():N}.TcPOU");
            var ex = Assert.Throws<FormatterException>(() => Runner().FormatFile(missing));
            Assert.Equal(-1, ex.ExitCode);
        }

        [Fact]
        public void FormatFiles_formats_multiple_files_in_one_invocation()
        {
            var a = NewTempFile(".TcDUT", MessyDut("A"));
            var b = NewTempFile(".TcDUT", MessyDut("B"));
            try
            {
                var outcome = Runner().FormatFiles(new[] { a, b });

                Assert.Equal(2, outcome.Changed);
                Assert.Equal(0, outcome.Unchanged);
                Assert.Empty(outcome.Failures);
                Assert.Contains("TYPE A : STRUCT", File.ReadAllText(a));
                Assert.Contains("TYPE B : STRUCT", File.ReadAllText(b));
            }
            finally { File.Delete(a); File.Delete(b); }
        }

        [Fact]
        public void FormatFiles_reports_unchanged_on_a_second_pass()
        {
            var a = NewTempFile(".TcDUT", MessyDut("A"));
            var b = NewTempFile(".TcDUT", MessyDut("B"));
            try
            {
                Runner().FormatFiles(new[] { a, b });               // first pass formats both
                var outcome = Runner().FormatFiles(new[] { a, b }); // second pass is a no-op

                Assert.Equal(0, outcome.Changed);
                Assert.Equal(2, outcome.Unchanged);
                Assert.Empty(outcome.Failures);
            }
            finally { File.Delete(a); File.Delete(b); }
        }

        [Fact]
        public void FormatFiles_does_not_abort_the_batch_on_a_single_parse_error()
        {
            var good = NewTempFile(".TcDUT", MessyDut("Good"));
            // A Unicode identifier the lexer rejects: this file errors (exit 3) and is left
            // untouched, but the batch must still format the good file and not throw.
            var bad = NewTempFile(".TcDUT", MessyDut("Bad").Replace("a:INT;", "café:INT;"));
            try
            {
                var outcome = Runner().FormatFiles(new[] { good, bad });

                Assert.Contains("TYPE Good : STRUCT", File.ReadAllText(good));
                Assert.Equal(1, outcome.Changed);   // the good file
                Assert.Equal(0, outcome.Unchanged);
                Assert.Single(outcome.Failures);    // the bad file, named and left untouched
                Assert.Equal(bad, outcome.Failures[0].Source); // the FULL path, not just the file name
                // The message is just tctidy's diagnostic: the path (already the Source) and tctidy's
                // own "tctidy:" prefix are not repeated in it.
                Assert.DoesNotContain(bad, outcome.Failures[0].Message);
                Assert.DoesNotContain("tctidy:", outcome.Failures[0].Message);
            }
            finally { File.Delete(good); File.Delete(bad); }
        }

        [Fact]
        public void FormatFiles_records_a_missing_file_as_a_failure()
        {
            var good = NewTempFile(".TcDUT", MessyDut("Good"));
            var missing = Path.Combine(Path.GetTempPath(), $"tctidy-missing-{Guid.NewGuid():N}.TcPOU");
            try
            {
                // The missing file fails the "before" read and is never handed to tctidy;
                // the good file is still formatted and the batch does not throw.
                var outcome = Runner().FormatFiles(new[] { good, missing });

                Assert.Contains("TYPE Good : STRUCT", File.ReadAllText(good));
                Assert.Equal(1, outcome.Changed);
                Assert.Single(outcome.Failures);
                Assert.Equal(missing, outcome.Failures[0].Source); // the FULL path of the unreadable file
            }
            finally { File.Delete(good); }
        }

        [Fact]
        public void FormatFiles_handles_a_large_batch_in_one_spawn()
        {
            // The path list is streamed on stdin, so a batch that would overflow the OS
            // command-line length limit as arguments still goes through in one invocation.
            const int count = 60;
            var files = new string[count];
            for (var i = 0; i < count; i++) files[i] = NewTempFile(".TcDUT", MessyDut("Big" + i));
            try
            {
                var outcome = Runner().FormatFiles(files);

                Assert.Equal(count, outcome.Changed);
                Assert.Empty(outcome.Failures);
            }
            finally { foreach (var f in files) File.Delete(f); }
        }

        [Fact]
        public void FormatFiles_handles_spaces_unicode_and_mixed_case_extensions()
        {
            var directory = Path.Combine(
                Path.GetTempPath(), $"tctidy test ä {Guid.NewGuid():N}");
            Directory.CreateDirectory(directory);
            var path = Path.Combine(directory, "Mixed Name.TcDuT");
            File.WriteAllText(path, MessyDut("MixedName"));
            try
            {
                var outcome = Runner().FormatFiles(new[] { path });

                Assert.Equal(1, outcome.Changed);
                Assert.Empty(outcome.Failures);
                Assert.Contains("TYPE MixedName : STRUCT", File.ReadAllText(path));
            }
            finally { Directory.Delete(directory, recursive: true); }
        }

        [Fact]
        public void FormatFiles_matches_diagnostic_paths_case_insensitively_on_windows()
        {
            if (!System.Runtime.InteropServices.RuntimeInformation.IsOSPlatform(
                    System.Runtime.InteropServices.OSPlatform.Windows))
            {
                return;
            }

            var actual = NewTempFile(
                ".TcDUT", MessyDut("Bad").Replace("a:INT;", "café:INT;"));
            var supplied = actual.ToUpperInvariant();
            try
            {
                var outcome = Runner().FormatFiles(new[] { supplied });

                Assert.Single(outcome.Failures);
                Assert.Equal(supplied, outcome.Failures[0].Source);
                Assert.Contains("parse error", outcome.Failures[0].Message);
            }
            finally { File.Delete(actual); }
        }

        [Fact]
        public void FormatFiles_matches_the_path_exactly_as_given_with_forward_slashes()
        {
            if (!System.Runtime.InteropServices.RuntimeInformation.IsOSPlatform(
                    System.Runtime.InteropServices.OSPlatform.Windows))
            {
                return;
            }

            // tctidy echoes each --files-from path as it was given, drive letter and separators
            // included, so the diagnostic is matched against that same spelling.
            var actual = NewTempFile(
                ".TcDUT", MessyDut("Bad").Replace("a:INT;", "café:INT;"));
            var supplied = actual.Replace('\\', '/');
            try
            {
                var outcome = Runner().FormatFiles(new[] { supplied });

                var failure = Assert.Single(outcome.Failures);
                Assert.Equal(supplied, failure.Source);
                Assert.StartsWith("parse error", failure.Message);
            }
            finally { File.Delete(actual); }
        }

        [Fact]
        public void FormatFiles_attributes_each_exit_3_diagnostic_to_its_own_file()
        {
            // Two refused files whose paths share a prefix, listed longer first, so the longer
            // file's stderr line comes first and also contains the shorter path.
            var root = NewTempDirectory();
            var good = Path.Combine(root, "Good.TcDUT");
            var shortBad = Path.Combine(root, "A.TcDUT");
            var longBad = Path.Combine(root, "A.TcDUT-suffix", "B.TcDUT");
            Directory.CreateDirectory(Path.GetDirectoryName(longBad)!);
            File.WriteAllText(good, MessyDut("Good"));
            File.WriteAllText(shortBad, MessyDut("A").Replace("a:INT;", "café:INT;"));
            File.WriteAllText(longBad, MessyDut("B").Replace("a:INT;", "café:INT;"));
            try
            {
                var outcome = Runner().FormatFiles(new[] { good, longBad, shortBad });

                Assert.Equal(1, outcome.Changed);
                Assert.Equal(0, outcome.Unchanged);
                Assert.Equal(new[] { longBad, shortBad }, outcome.Failures.Select(f => f.Source));
                Assert.All(outcome.Failures, f => Assert.StartsWith("parse error", f.Message));
            }
            finally { Directory.Delete(root, recursive: true); }
        }

        [Fact]
        public void FormatFiles_does_not_give_a_prefix_path_the_diagnostic_of_a_longer_one()
        {
            var root = NewTempDirectory();
            var clean = Path.Combine(root, "A.TcDUT");
            var formatted = Path.Combine(root, "A.TcDUT-suffix", "Messy.TcDUT");
            var refused = Path.Combine(root, "A.TcDUT-suffix", "Bad.TcDUT");
            Directory.CreateDirectory(Path.GetDirectoryName(formatted)!);
            File.WriteAllText(clean, MessyDut("A"));
            File.WriteAllText(formatted, MessyDut("Messy"));
            File.WriteAllText(refused, MessyDut("Bad").Replace("a:INT;", "café:INT;"));
            try
            {
                Runner().FormatFiles(new[] { clean }); // already formatted before the batch

                // The batch exits 3, and both "formatted <longer path>" and "<longer path>: parse
                // error" contain the clean file's path: neither may be read as its diagnostic.
                var outcome = Runner().FormatFiles(new[] { clean, formatted, refused });

                Assert.Equal(1, outcome.Changed);
                Assert.Equal(1, outcome.Unchanged);
                var failure = Assert.Single(outcome.Failures);
                Assert.Equal(refused, failure.Source);
                Assert.StartsWith("parse error", failure.Message);
            }
            finally { Directory.Delete(root, recursive: true); }
        }

        [Fact]
        public void FormatFiles_reports_every_file_when_tctidy_cannot_start()
        {
            var a = NewTempFile(".TcDUT", MessyDut("A"));
            var b = NewTempFile(".TcDUT", MessyDut("B"));
            var before = File.ReadAllBytes(a);
            try
            {
                var runner = new TctidyRunner(
                    Path.Combine(Path.GetTempPath(), "tctidy-does-not-exist"), TimeSpan.FromSeconds(5));

                var outcome = runner.FormatFiles(new[] { a, b });

                Assert.Equal(0, outcome.Changed);
                Assert.Equal(0, outcome.Unchanged);
                Assert.Equal(new[] { a, b }, outcome.Failures.Select(f => f.Source));
                Assert.All(outcome.Failures, f => Assert.Contains("could not start tctidy", f.Message));
                Assert.Equal(before, File.ReadAllBytes(a));
            }
            finally { File.Delete(a); File.Delete(b); }
        }

        [Fact]
        public void FormatFiles_keeps_formatted_files_and_fails_the_rest_when_a_config_is_invalid()
        {
            // tctidy exits 2 but still formats the files whose configuration is valid. The files
            // it left alone cannot be told apart from clean ones, so none is reported unchanged.
            var root = NewTempDirectory();
            var good = Path.Combine(root, "Good.TcDUT");
            var broken = Path.Combine(root, "Broken");
            var skipped = Path.Combine(broken, "Skipped.TcDUT");
            Directory.CreateDirectory(broken);
            File.WriteAllText(good, MessyDut("Good"));
            File.WriteAllText(Path.Combine(broken, ".tctidy.toml"), "bogus_option = 1\n");
            File.WriteAllText(skipped, MessyDut("Skipped"));
            var skippedBefore = File.ReadAllBytes(skipped);
            try
            {
                var outcome = Runner().FormatFiles(new[] { good, skipped });

                Assert.Equal(1, outcome.Changed);
                Assert.Equal(0, outcome.Unchanged);
                var failure = Assert.Single(outcome.Failures);
                Assert.Equal(skipped, failure.Source);
                Assert.Contains("exit code 2", failure.Message);
                Assert.Contains("unknown option", failure.Message);
                Assert.Contains("TYPE Good : STRUCT", File.ReadAllText(good));
                Assert.Equal(skippedBefore, File.ReadAllBytes(skipped));
            }
            finally { Directory.Delete(root, recursive: true); }
        }

        [Fact]
        public void FormatFiles_surfaces_a_timeout_without_rewriting_the_file()
        {
            var body = new System.Text.StringBuilder(capacity: 1_600_000);
            for (var i = 0; i < 200_000; i++) body.Append("x:=1;\r\n");
            var path = NewTempFile(".st", body.ToString());
            var before = File.ReadAllBytes(path);
            try
            {
                var runner = new TctidyRunner(TctidyPath(), TimeSpan.FromMilliseconds(1));

                // A timeout is a failure of the file, not an exception that loses the batch.
                var outcome = runner.FormatFiles(new[] { path });

                Assert.Equal(0, outcome.Changed);
                Assert.Equal(0, outcome.Unchanged);
                var failure = Assert.Single(outcome.Failures);
                Assert.Equal(path, failure.Source);
                Assert.Contains("timed out", failure.Message);
                Assert.Equal(before, File.ReadAllBytes(path));
            }
            finally { File.Delete(path); }
        }

        [Fact]
        public void Missing_binary_throws()
        {
            var runner = new TctidyRunner(
                Path.Combine(Path.GetTempPath(), "tctidy-does-not-exist"),
                TimeSpan.FromSeconds(5));
            Assert.ThrowsAny<Exception>(
                () => runner.FormatText("x:=1;", StSection.Implementation));
        }

        private static string NewTempFile(string extension, string content)
        {
            var path = Path.Combine(Path.GetTempPath(), $"tctidy-test-{Guid.NewGuid():N}{extension}");
            File.WriteAllText(path, content);
            return path;
        }

        private static string NewTempDirectory()
        {
            var path = Path.Combine(Path.GetTempPath(), $"tctidy-test-{Guid.NewGuid():N}");
            Directory.CreateDirectory(path);
            return path;
        }

        // A messy (lower-cased keywords) .TcDUT that formats to "TYPE <name> : STRUCT …".
        private static string MessyDut(string name) =>
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n" +
            "<TcPlcObject Version=\"1.1.0.1\">\r\n" +
            $"  <DUT Name=\"{name}\" Id=\"{{00000000-0000-0000-0000-000000000000}}\">\r\n" +
            $"    <Declaration><![CDATA[tYpE {name} : sTrUcT\r\na:INT;\r\neNd_sTrUcT\r\neNd_tYpE]]></Declaration>\r\n" +
            "  </DUT>\r\n" +
            "</TcPlcObject>\r\n";
    }
}
