using System;
using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    /// <summary>
    /// Exercises <see cref="FormatReport"/>, the pure renderer of the "Format with TcTidy" run summary
    /// and the per-file failure list written to the Output pane.
    /// </summary>
    public sealed class FormatReportTests
    {
        [Fact]
        public void Format_lists_the_summary_line_with_counts()
        {
            var report = new FormatReport(2, 5, Array.Empty<FileFailure>(), apiCount: 1, diskCount: 6);

            Assert.Equal("TcTidy: 2 formatted, 5 unchanged (1 open via API, 6 on disk)", report.Format());
        }

        [Fact]
        public void Summary_is_a_single_line_and_matches_the_first_line_of_the_report()
        {
            var report = new FormatReport(
                0, 0, new[] { new FileFailure(@"C:\plc\A.TcDUT", "boom") }, apiCount: 0, diskCount: 1);

            Assert.Equal("TcTidy: 0 formatted, 0 unchanged, 1 failed (0 open via API, 1 on disk)", report.Summary);
            Assert.DoesNotContain(Environment.NewLine, report.Summary);
            Assert.StartsWith(report.Summary, report.Format()); // the pane's first line is the summary
        }

        [Fact]
        public void Format_appends_the_failed_count_when_there_are_failures()
        {
            var report = new FormatReport(
                0, 0, new[] { new FileFailure(@"C:\plc\A.TcDUT", "1:1: parse error") }, apiCount: 0, diskCount: 1);

            Assert.Contains("0 formatted, 0 unchanged, 1 failed (0 open via API, 1 on disk)", report.Format());
        }

        [Fact]
        public void Format_lists_each_failure_on_its_own_line_with_full_path_and_message()
        {
            var failures = new[]
            {
                new FileFailure(@"C:\plc\A.TcDUT", "1:1: parse error"),
                new FileFailure(@"C:\plc\sub\B.TcDUT", "2:5: unexpected token"),
            };
            var report = new FormatReport(0, 0, failures, apiCount: 0, diskCount: 2);

            var lines = report.Format().Split(new[] { Environment.NewLine }, StringSplitOptions.None);

            Assert.Contains(@"  C:\plc\A.TcDUT: 1:1: parse error", lines);
            Assert.Contains(@"  C:\plc\sub\B.TcDUT: 2:5: unexpected token", lines);
            // Exactly the two failures are listed (one indented line each).
            Assert.Equal(2, Array.FindAll(lines, l => l.StartsWith("  ")).Length);
        }

        [Fact]
        public void Format_shows_a_left_untouched_header_only_when_there_are_failures()
        {
            var clean = new FormatReport(1, 0, Array.Empty<FileFailure>(), apiCount: 1, diskCount: 0);
            Assert.DoesNotContain("left untouched", clean.Format());

            var withFailure = new FormatReport(
                0, 0, new[] { new FileFailure(@"C:\plc\A.TcDUT", "boom") }, apiCount: 0, diskCount: 1);
            Assert.Contains("These files were left untouched:", withFailure.Format());
        }

        [Fact]
        public void FailureCount_reflects_the_failure_list()
        {
            var failures = new[]
            {
                new FileFailure("A", "e1"),
                new FileFailure("B", "e2"),
            };
            Assert.Equal(2, new FormatReport(0, 0, failures, 0, 2).FailureCount);
            Assert.Equal(0, new FormatReport(3, 4, Array.Empty<FileFailure>(), 0, 7).FailureCount);
        }

        [Fact]
        public void FailureNotice_is_empty_for_a_clean_run()
        {
            var report = new FormatReport(3, 4, Array.Empty<FileFailure>(), apiCount: 1, diskCount: 6);

            Assert.Equal(string.Empty, report.FailureNotice);
        }

        [Fact]
        public void FailureNotice_names_a_single_failure_in_the_singular()
        {
            var report = new FormatReport(
                0, 0, new[] { new FileFailure(@"C:\plc\A.TcDUT", "boom") }, apiCount: 0, diskCount: 1);

            Assert.Equal(
                "TcTidy left 1 file untouched. See the \"TcTidy\" Output pane for the per-file details.",
                report.FailureNotice);
        }

        [Fact]
        public void FailureNotice_names_multiple_failures_in_the_plural()
        {
            var failures = new[]
            {
                new FileFailure("A", "e1"),
                new FileFailure("B", "e2"),
                new FileFailure("C", "e3"),
            };
            var report = new FormatReport(0, 0, failures, apiCount: 0, diskCount: 3);

            Assert.Equal(
                "TcTidy left 3 files untouched. See the \"TcTidy\" Output pane for the per-file details.",
                report.FailureNotice);
        }

        [Fact]
        public void FailureNotice_points_to_the_output_pane()
        {
            var report = new FormatReport(
                0, 0, new[] { new FileFailure(@"C:\plc\A.TcDUT", "boom") }, apiCount: 0, diskCount: 1);

            Assert.Contains("Output pane", report.FailureNotice);
        }
    }
}
