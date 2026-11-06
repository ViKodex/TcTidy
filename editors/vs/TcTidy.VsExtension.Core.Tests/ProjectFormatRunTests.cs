using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    /// <summary>
    /// Exercises <see cref="ProjectFormatRun"/>, the two-phase project-wide run: whatever a phase
    /// does, the progress feedback stops once and the run ends in one report that keeps what the
    /// other phase achieved.
    /// </summary>
    public sealed class ProjectFormatRunTests
    {
        private static readonly string[] OpenPous = { "FB_A", "FB_B" };
        private static readonly string[] DiskFiles = { @"C:\plc\C.TcPOU", @"C:\plc\D.TcPOU" };

        [Fact]
        public async Task A_disk_phase_that_throws_keeps_the_open_results_and_fails_each_disk_file()
        {
            var progress = new RecordingProgress();

            var result = await ProjectFormatRun.ExecuteAsync(
                progress,
                OpenPous, () => new BatchOutcome(1, 1, Array.Empty<FileFailure>()),
                DiskFiles, () => throw new FormatterException(-1, "could not start tctidy ('x'): not found"));

            Assert.Equal(new[] { "start 4", "open 2", "disk 2", "end" }, progress.Calls);
            Assert.Equal(
                "TcTidy: 1 formatted, 1 unchanged, 2 failed (2 open via API, 2 on disk)",
                result.Report.Summary);
            Assert.Equal(DiskFiles, result.Disk.Failures.Select(f => f.Source));
            Assert.All(result.Disk.Failures, f => Assert.Contains("could not start tctidy", f.Message));
        }

        [Fact]
        public async Task A_disk_phase_that_faults_later_is_reported_the_same_way()
        {
            var progress = new RecordingProgress();

            var result = await ProjectFormatRun.ExecuteAsync(
                progress,
                OpenPous, () => new BatchOutcome(2, 0, Array.Empty<FileFailure>()),
                DiskFiles, async () =>
                {
                    await Task.Yield();
                    throw new InvalidOperationException("pipe closed");
                });

            Assert.Equal("end", progress.Calls.Last());
            Assert.Equal(2, result.Changed);
            Assert.Equal(DiskFiles, result.Failures.Select(f => f.Source));
            Assert.All(result.Failures, f => Assert.Contains("pipe closed", f.Message));
        }

        [Fact]
        public async Task An_open_phase_that_throws_still_formats_the_disk_files()
        {
            var progress = new RecordingProgress();

            var result = await ProjectFormatRun.ExecuteAsync(
                progress,
                OpenPous, () => throw new InvalidOperationException("COM boom"),
                DiskFiles, () => Task.FromResult(new BatchOutcome(2, 0, Array.Empty<FileFailure>())));

            Assert.Equal(new[] { "start 4", "open 2", "disk 2", "end" }, progress.Calls);
            Assert.Equal(2, result.Disk.Changed);
            Assert.Equal(OpenPous, result.Open.Failures.Select(f => f.Source));
            Assert.All(result.Open.Failures, f => Assert.Contains("COM boom", f.Message));
            Assert.Equal(
                "TcTidy: 2 formatted, 0 unchanged, 2 failed (2 open via API, 2 on disk)",
                result.Report.Summary);
        }

        [Fact]
        public async Task Progress_that_throws_neither_stops_the_run_nor_skips_the_end()
        {
            var progress = new RecordingProgress { ThrowOnUpdates = true };

            var result = await ProjectFormatRun.ExecuteAsync(
                progress,
                OpenPous, () => new BatchOutcome(2, 0, Array.Empty<FileFailure>()),
                DiskFiles, () => Task.FromResult(new BatchOutcome(1, 1, Array.Empty<FileFailure>())));

            Assert.Equal(1, progress.Calls.Count(c => c == "end"));
            Assert.Equal("TcTidy: 3 formatted, 1 unchanged (2 open via API, 2 on disk)", result.Report.Summary);
        }

        [Fact]
        public async Task An_empty_phase_is_skipped()
        {
            var progress = new RecordingProgress();
            var openRan = false;

            var result = await ProjectFormatRun.ExecuteAsync(
                progress,
                Array.Empty<string>(), () => { openRan = true; return new BatchOutcome(0, 0, Array.Empty<FileFailure>()); },
                DiskFiles, () => Task.FromResult(new BatchOutcome(0, 2, Array.Empty<FileFailure>())));

            Assert.False(openRan);
            Assert.Equal(new[] { "start 2", "disk 2", "end" }, progress.Calls);
            Assert.Equal("TcTidy: 0 formatted, 2 unchanged (0 open via API, 2 on disk)", result.Report.Summary);
        }

        private sealed class RecordingProgress : IRunProgress
        {
            public List<string> Calls { get; } = new List<string>();
            public bool ThrowOnUpdates { get; set; }

            public Task StartAsync(int total) => Record("start " + total);
            public Task OpenPhaseAsync(int count) => Record("open " + count);
            public Task DiskPhaseAsync(int count) => Record("disk " + count);

            public Task EndAsync()
            {
                Calls.Add("end");
                return Task.CompletedTask;
            }

            private Task Record(string call)
            {
                Calls.Add(call);
                if (ThrowOnUpdates) throw new InvalidOperationException("status bar unavailable");
                return Task.CompletedTask;
            }
        }
    }
}
