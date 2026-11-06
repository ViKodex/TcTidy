using System;
using System.Globalization;
using System.IO;
using System.Threading.Tasks;
using TcTidy.VsExtension.Core;

namespace TcTidy.VsExtension
{
    /// <summary>
    /// Test-mode timing sink for the end-to-end "Format with TcTidy" cost. Toggled from the
    /// TcTidy menu (Performance Report, persisted in <see cref="GeneralOptions"/>, effective on
    /// the next run); off by default, so normal use is untouched. When on, each run writes a
    /// precise <see cref="PerfReport"/> to the "TcTidy" Output pane and appends it to
    /// <c>%TEMP%\tctidy-perf.log</c>. Both writes are best-effort, diagnostics never break a format.
    /// </summary>
    internal static class PerfMode
    {
        public static bool Enabled => GeneralOptions.Instance.PerfReportEnabled;

        public static async Task ReportAsync(PerfReport report)
        {
            var stamp = DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss", CultureInfo.InvariantCulture);
            var text = "[" + stamp + "] " + report.Format();

            // A log file is the robust, persistent record, it survives even if the Output API
            // misbehaves, and it accumulates runs so a test session can be analysed afterwards.
            try
            {
                var log = Path.Combine(Path.GetTempPath(), "tctidy-perf.log");
                File.AppendAllText(log, text + Environment.NewLine + Environment.NewLine);
            }
            catch
            {
                // best effort
            }

            // The Output pane is the idiomatic place to read it inside the IDE; it shares the one
            // cached pane with the run report. Perf mode is an opt-in diagnostic session, so the
            // pane is brought to the front after each run rather than left buried behind a sibling
            // tab; a normal run only surfaces it on failure. This runs after the run report's own
            // dialog/activation, so it never fights a modal.
            await TcTidyOutput.WriteLineAsync(text);
            await TcTidyOutput.ActivateAsync();
        }
    }
}
