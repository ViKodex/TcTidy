using System;
using System.Linq;
using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    public sealed class PerfReportTests
    {
        [Fact]
        public void Format_lists_counts_phases_and_a_per_path_breakdown()
        {
            var report = new PerfReport { Files = 47, Changed = 12, Unchanged = 35, Failed = 0 };
            report.Add("Detect files", TimeSpan.FromMilliseconds(18.5));
            report.AddFormat("Format", TimeSpan.FromMilliseconds(940.0), 47, 12, 35, indented: false);
            report.AddFormat("Open via API", TimeSpan.FromMilliseconds(600.0), 3, 3, 0, indented: true);
            report.AddFormat("Disk batch", TimeSpan.FromMilliseconds(340.0), 44, 9, 35, indented: true);
            report.Add("Total (end-to-end)", TimeSpan.FromMilliseconds(1080.0));

            var text = report.Format();

            Assert.Contains("47 file(s): 12 changed, 35 unchanged, 0 failed", text);
            Assert.Contains("Detect files", text);
            Assert.Contains("18.50 ms", text);
            // The Format total shows the file count, not a (misleading) blended per-file average.
            Assert.Contains("47 files", text);
            Assert.DoesNotContain("20.00 ms/file", text); // 940 ms / 47, the old blended average is gone
            // A sub-phase shows its per-file average and its own changed/unchanged split.
            Assert.Contains("Open via API", text);
            Assert.Contains("200.00 ms/file", text); // 600 ms / 3 files
            Assert.Contains("3 changed, 0 unchanged", text);
            Assert.Contains("Total (end-to-end)", text);
        }

        [Fact]
        public void Phases_preserve_insertion_order()
        {
            var report = new PerfReport();
            report.Add("a", TimeSpan.Zero);
            report.Add("b", TimeSpan.Zero);
            report.Add("c", TimeSpan.Zero);

            Assert.Equal(new[] { "a", "b", "c" }, report.Phases.Select(p => p.Key));
        }

        [Fact]
        public void No_per_file_average_when_a_phase_has_no_files()
        {
            var report = new PerfReport();
            report.AddFormat("Open via API", TimeSpan.FromMilliseconds(5), 0, 0, 0, indented: true);

            var text = report.Format();
            Assert.Contains("0 files", text);
            Assert.DoesNotContain("ms/file", text);
        }
    }
}
