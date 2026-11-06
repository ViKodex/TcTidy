using System.Linq;
using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    /// <summary>
    /// Exercises <see cref="DiskBatchResult"/>: how a <c>tctidy --files-from -</c> run that exited
    /// with any code, or never exited, turns into changed, unchanged and failed files.
    /// </summary>
    public sealed class DiskBatchResultTests
    {
        private const string A = @"C:\plc\A.TcDUT";
        private const string B = @"C:\plc\B.TcDUT";
        private const string C = @"C:\plc\C.TcDUT";
        private const string D = @"C:\plc\D.TcDUT";

        [Fact]
        public void Exit_0_counts_changed_and_unchanged_files()
        {
            var outcome = DiskBatchResult.Build(
                new[] { (A, true), (B, false) },
                EngineRun.Exited(0, "tctidy: formatted " + A + "\n"),
                windowsPaths: true);

            Assert.Equal(1, outcome.Changed);
            Assert.Equal(1, outcome.Unchanged);
            Assert.Empty(outcome.Failures);
        }

        [Fact]
        public void Exit_3_fails_exactly_the_files_tctidy_named()
        {
            var stderr =
                "tctidy: formatted " + A + "\n" +
                "tctidy: " + B + ": parse error in declaration: line 2\n" +
                "tctidy: " + D + ": parse error in implementation: line 7\n";

            var outcome = DiskBatchResult.Build(
                new[] { (A, true), (B, false), (C, false), (D, false) },
                EngineRun.Exited(3, stderr),
                windowsPaths: true);

            Assert.Equal(1, outcome.Changed);
            Assert.Equal(1, outcome.Unchanged); // C: already clean
            Assert.Equal(new[] { B, D }, outcome.Failures.Select(f => f.Source));
            Assert.Equal("parse error in declaration: line 2", outcome.Failures[0].Message);
            Assert.Equal("parse error in implementation: line 7", outcome.Failures[1].Message);
        }

        [Fact]
        public void A_spawn_failure_fails_every_file()
        {
            var outcome = DiskBatchResult.Build(
                new[] { (A, false), (B, false) },
                EngineRun.Aborted("could not start tctidy ('tctidy.exe'): not found"),
                windowsPaths: true);

            Assert.Equal(0, outcome.Changed);
            Assert.Equal(0, outcome.Unchanged);
            Assert.Equal(new[] { A, B }, outcome.Failures.Select(f => f.Source));
            Assert.All(outcome.Failures, f =>
                Assert.Equal("could not start tctidy ('tctidy.exe'): not found", f.Message));
        }

        [Fact]
        public void A_timeout_keeps_the_files_tctidy_already_rewrote()
        {
            var outcome = DiskBatchResult.Build(
                new[] { (A, true), (B, false) },
                EngineRun.Aborted("tctidy timed out after 60s"),
                windowsPaths: true);

            Assert.Equal(1, outcome.Changed);
            Assert.Equal(0, outcome.Unchanged);
            var failure = Assert.Single(outcome.Failures);
            Assert.Equal(B, failure.Source);
            Assert.Equal("tctidy timed out after 60s", failure.Message);
        }

        [Fact]
        public void A_usage_or_config_error_fails_every_unconfirmed_file_with_tctidys_message()
        {
            var stderr =
                "tctidy: formatted " + A + "\n" +
                @"tctidy: C:\plc\sub\.tctidy.toml:1:1: unknown option `x`" + "\n";

            var outcome = DiskBatchResult.Build(
                new[] { (A, true), (B, false) }, EngineRun.Exited(2, stderr), windowsPaths: true);

            Assert.Equal(1, outcome.Changed);
            Assert.Equal(0, outcome.Unchanged);
            var failure = Assert.Single(outcome.Failures);
            Assert.Equal(B, failure.Source);
            Assert.Equal(
                @"tctidy failed (exit code 2): C:\plc\sub\.tctidy.toml:1:1: unknown option `x`",
                failure.Message);
        }

        [Fact]
        public void An_unexpected_exit_fails_every_unconfirmed_file()
        {
            var panicked = DiskBatchResult.Build(
                new[] { (A, false) },
                EngineRun.Exited(101, "thread 'main' panicked at src\\main.rs:1:1:\nboom\n"),
                windowsPaths: true);
            var crashed = DiskBatchResult.Build(
                new[] { (A, false) }, EngineRun.Exited(-1073741819, string.Empty), windowsPaths: true);

            Assert.Equal(
                "tctidy failed (exit code 101): thread 'main' panicked at src\\main.rs:1:1:; boom",
                Assert.Single(panicked.Failures).Message);
            Assert.Equal(
                "tctidy failed (exit code -1073741819)", Assert.Single(crashed.Failures).Message);
        }

        [Fact]
        public void A_file_named_in_a_failed_run_keeps_its_own_diagnostic()
        {
            var stderr =
                "tctidy: " + B + ": parse error\n" +
                @"tctidy: C:\plc\sub\.tctidy.toml:1:1: unknown option `x`" + "\n";

            var outcome = DiskBatchResult.Build(
                new[] { (B, false), (C, false) }, EngineRun.Exited(2, stderr), windowsPaths: true);

            Assert.Equal(new[] { B, C }, outcome.Failures.Select(f => f.Source));
            Assert.Equal("parse error", outcome.Failures[0].Message);
            Assert.StartsWith("tctidy failed (exit code 2): ", outcome.Failures[1].Message);
        }

        [Fact]
        public void Exit_3_does_not_fail_a_prefix_path_for_a_longer_paths_line()
        {
            const string prefix = @"C:\plc\A.TcDUT";
            const string longer = @"C:\plc\A.TcDUT-suffix\B.TcDUT";

            var outcome = DiskBatchResult.Build(
                new[] { (prefix, false), (longer, false) },
                EngineRun.Exited(3, "tctidy: " + longer + ": parse error\n"),
                windowsPaths: true);

            Assert.Equal(1, outcome.Unchanged);
            Assert.Equal(longer, Assert.Single(outcome.Failures).Source);
        }
    }
}
