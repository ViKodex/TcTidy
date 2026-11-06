using System;
using System.Collections.Generic;
using System.Text;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// Renders one "Format with TcTidy" run into the text written to the "TcTidy" Output pane: a
    /// summary line (its wording kept identical to the status-bar summary) and, when files were refused,
    /// one line per failure with its full path (or qualified element name) and tctidy's diagnostic, so
    /// each failure is surfaced individually instead of aggregated into a single dialog. Pure and VS-free
    /// so it is unit-testable; the extension renders <see cref="Format"/> and writes it to the pane.
    /// </summary>
    public sealed class FormatReport
    {
        private readonly int _changed;
        private readonly int _unchanged;
        private readonly IReadOnlyList<FileFailure> _failures;
        private readonly int _apiCount;
        private readonly int _diskCount;

        public FormatReport(
            int changed, int unchanged, IReadOnlyList<FileFailure> failures, int apiCount, int diskCount)
        {
            _changed = changed;
            _unchanged = unchanged;
            _failures = failures ?? throw new ArgumentNullException(nameof(failures));
            _apiCount = apiCount;
            _diskCount = diskCount;
        }

        /// <summary>How many files were left untouched, lets the caller decide whether to surface the
        /// pane without re-counting.</summary>
        public int FailureCount => _failures.Count;

        /// <summary>A short, hard-to-miss notice for the warning dialog: how many files were left
        /// untouched and where the per-file details live. Empty for a clean run, no dialog is shown
        /// then. The full list stays in the pane (<see cref="Format"/>); this is only a pointer.</summary>
        public string FailureNotice
        {
            get
            {
                if (_failures.Count == 0) return string.Empty;
                var noun = _failures.Count == 1 ? "file" : "files";
                return $"TcTidy left {_failures.Count} {noun} untouched. " +
                    "See the \"TcTidy\" Output pane for the per-file details.";
            }
        }

        /// <summary>The one-line run summary, also shown in the status bar, the single source of that
        /// wording, so the pane's first line and the status bar can never drift apart.</summary>
        public string Summary
        {
            get
            {
                var sb = new StringBuilder();
                sb.Append("TcTidy: ").Append(_changed).Append(" formatted, ")
                    .Append(_unchanged).Append(" unchanged");
                if (_failures.Count > 0) sb.Append(", ").Append(_failures.Count).Append(" failed");
                sb.Append(" (").Append(_apiCount).Append(" open via API, ")
                    .Append(_diskCount).Append(" on disk)");
                return sb.ToString();
            }
        }

        /// <summary>The full pane text: the <see cref="Summary"/>, then (when files were refused) a header
        /// and one line per failure with its source and tctidy's diagnostic.</summary>
        public string Format()
        {
            if (_failures.Count == 0) return Summary;

            var sb = new StringBuilder(Summary);
            sb.Append(Environment.NewLine).Append("These files were left untouched:");
            foreach (var failure in _failures)
            {
                sb.Append(Environment.NewLine).Append("  ").Append(failure.ToString());
            }
            return sb.ToString();
        }
    }
}
