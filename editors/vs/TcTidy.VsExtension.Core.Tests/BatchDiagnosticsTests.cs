using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    /// <summary>
    /// Exercises <see cref="BatchDiagnostics"/>, which reads the per-file lines of a
    /// <c>tctidy --files-from -</c> run. tctidy echoes each path as it was given, in the shape
    /// <c>tctidy: &lt;path&gt;: &lt;message&gt;</c>, so a line belongs to a file only when it names
    /// that complete path.
    /// </summary>
    public sealed class BatchDiagnosticsTests
    {
        [Fact]
        public void Matches_a_windows_path_with_its_drive_letter_and_backslashes()
        {
            const string path = @"C:\plc\POUs\FB_A.TcPOU";
            var diagnostics = BatchDiagnostics.Parse(
                "tctidy: " + path + ": parse error in implementation: line 3\n",
                new[] { path }, windowsPaths: true);

            Assert.True(diagnostics.TryGetMessage(path, out var message));
            Assert.Equal("parse error in implementation: line 3", message);
        }

        [Fact]
        public void A_prefix_path_does_not_take_the_lines_of_a_longer_one()
        {
            const string prefix = @"C:\plc\A.TcDUT";
            const string refused = @"C:\plc\A.TcDUT-suffix\B.TcDUT";
            const string formatted = @"C:\plc\A.TcDUT-suffix\C.TcDUT";
            var stderr =
                "tctidy: formatted " + formatted + "\n" +
                "tctidy: " + refused + ": parse error\n";

            var diagnostics = BatchDiagnostics.Parse(
                stderr, new[] { prefix, refused, formatted }, windowsPaths: true);

            Assert.False(diagnostics.TryGetMessage(prefix, out _));
            Assert.False(diagnostics.TryGetMessage(formatted, out _));
            Assert.True(diagnostics.TryGetMessage(refused, out var message));
            Assert.Equal("parse error", message);
        }

        [Fact]
        public void A_suffix_path_does_not_take_the_lines_of_a_longer_one()
        {
            const string tail = "/plc/A.TcDUT";
            const string refused = "/old/plc/A.TcDUT";

            var diagnostics = BatchDiagnostics.Parse(
                "tctidy: " + refused + ": parse error\n", new[] { tail, refused }, windowsPaths: false);

            Assert.False(diagnostics.TryGetMessage(tail, out _));
            Assert.True(diagnostics.TryGetMessage(refused, out var message));
            Assert.Equal("parse error", message);
        }

        [Fact]
        public void Windows_paths_match_regardless_of_case_and_separator()
        {
            // The file system treats these spellings as one file, and so does tctidy, which formats
            // and reports such a file once, under the first spelling it was given.
            var diagnostics = BatchDiagnostics.Parse(
                "tctidy: c:/plc/a.tcdut: parse error\n", new[] { @"C:\PLC\A.TcDUT" }, windowsPaths: true);

            Assert.True(diagnostics.TryGetMessage(@"C:\PLC\A.TcDUT", out var message));
            Assert.Equal("parse error", message);
        }

        [Fact]
        public void Other_paths_match_case_sensitively()
        {
            var diagnostics = BatchDiagnostics.Parse(
                "tctidy: /plc/a.tcdut: parse error\n", new[] { "/plc/A.TcDUT" }, windowsPaths: false);

            Assert.False(diagnostics.TryGetMessage("/plc/A.TcDUT", out _));
        }

        [Fact]
        public void Keeps_the_first_line_of_a_file()
        {
            const string path = @"C:\plc\A.TcDUT";
            var diagnostics = BatchDiagnostics.Parse(
                "tctidy: " + path + ": first\r\ntctidy: " + path + ": second\r\n",
                new[] { path }, windowsPaths: true);

            Assert.True(diagnostics.TryGetMessage(path, out var message));
            Assert.Equal("first", message);
        }

        [Fact]
        public void Lines_that_name_no_target_are_kept_for_the_run()
        {
            const string path = @"C:\plc\A.TcDUT";
            var stderr =
                "tctidy: formatted " + path + "\r\n" +
                @"tctidy: C:\plc\sub\.tctidy.toml:1:1: unknown option `x`" + "\r\n" +
                "\r\n" +
                @"thread 'main' panicked at src\main.rs:1:1:" + "\r\n";

            var diagnostics = BatchDiagnostics.Parse(stderr, new[] { path }, windowsPaths: true);

            Assert.False(diagnostics.TryGetMessage(path, out _));
            Assert.Equal(
                new[]
                {
                    @"C:\plc\sub\.tctidy.toml:1:1: unknown option `x`",
                    @"thread 'main' panicked at src\main.rs:1:1:",
                },
                diagnostics.OtherLines);
        }
    }
}
