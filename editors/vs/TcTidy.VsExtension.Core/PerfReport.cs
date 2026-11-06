using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// A precise, end-to-end timing report for one "Format with TcTidy" invocation: the wall-clock
    /// of each phase plus the file counts. Format phases can carry a per-file breakdown (file count,
    /// and, for the "open via API" vs "disk batch" sub-phases, a per-file average and their own
    /// changed/unchanged split). Pure and VS-free so it is unit-testable; the extension fills it from
    /// <see cref="System.Diagnostics.Stopwatch"/> measurements and renders <see cref="Format"/> to the
    /// Output window / a log file when perf mode is on (see the extension's PerfMode).
    /// </summary>
    public sealed class PerfReport
    {
        private readonly List<Phase> _phases = new List<Phase>();

        public int Files { get; set; }
        public int Changed { get; set; }
        public int Unchanged { get; set; }
        public int Failed { get; set; }

        /// <summary>Record a simple named phase's elapsed time, in call order.</summary>
        public void Add(string label, TimeSpan elapsed) =>
            _phases.Add(new Phase(label, elapsed, indented: false, files: null, changed: 0, unchanged: 0));

        /// <summary>
        /// Record a format phase with a per-file breakdown. <paramref name="indented"/> marks the
        /// "open via API" / "disk batch" sub-phases, which print a per-file average and their
        /// changed/unchanged split; the un-indented total shows the file count only (a blended
        /// per-file average across both paths would mislead).
        /// </summary>
        public void AddFormat(string label, TimeSpan elapsed, int files, int changed, int unchanged, bool indented) =>
            _phases.Add(new Phase(label, elapsed, indented, files, changed, unchanged));

        /// <summary>The recorded phase labels and times, in the order they were added.</summary>
        public IReadOnlyList<KeyValuePair<string, TimeSpan>> Phases =>
            _phases.Select(p => new KeyValuePair<string, TimeSpan>(p.Label, p.Elapsed)).ToList();

        /// <summary>
        /// Render a stable, human-readable report: a summary line, then one phase per line with
        /// right-aligned milliseconds; format phases also show their file count, and the indented
        /// sub-phases their per-file average and changed/unchanged split.
        /// </summary>
        public string Format()
        {
            var sb = new StringBuilder();
            sb.Append("TcTidy perf, ").Append(Files).Append(" file(s): ")
                .Append(Changed).Append(" changed, ")
                .Append(Unchanged).Append(" unchanged, ")
                .Append(Failed).Append(" failed");

            foreach (var p in _phases)
            {
                // Sub-phases are indented one extra level; the label pad is reduced to match so the
                // " : " column still lines up with the top-level phases.
                var indent = p.Indented ? "    " : "  ";
                var pad = p.Indented ? 20 : 22;
                sb.Append(Environment.NewLine).Append(indent)
                    .Append(p.Label.PadRight(pad)).Append(" : ").Append(Ms(p.Elapsed));

                if (!p.Files.HasValue) continue;

                sb.Append("   ").Append(p.Files.Value).Append(p.Files.Value == 1 ? " file" : " files");

                // Per-file cost and the changed/unchanged split are only meaningful for a single
                // path; the blended total shows the count only.
                if (p.Indented && p.Files.Value > 0)
                {
                    var perFile = p.Elapsed.TotalMilliseconds / p.Files.Value;
                    sb.Append("   ").Append(perFile.ToString("0.00", CultureInfo.InvariantCulture)).Append(" ms/file")
                        .Append("   ").Append(p.Changed).Append(" changed, ")
                        .Append(p.Unchanged).Append(" unchanged");
                }
            }

            return sb.ToString();
        }

        private static string Ms(TimeSpan t) =>
            t.TotalMilliseconds.ToString("0.00", CultureInfo.InvariantCulture).PadLeft(10) + " ms";

        private sealed class Phase
        {
            public Phase(string label, TimeSpan elapsed, bool indented, int? files, int changed, int unchanged)
            {
                Label = label;
                Elapsed = elapsed;
                Indented = indented;
                Files = files;
                Changed = changed;
                Unchanged = unchanged;
            }

            public string Label { get; }
            public TimeSpan Elapsed { get; }
            public bool Indented { get; }
            public int? Files { get; }
            public int Changed { get; }
            public int Unchanged { get; }
        }
    }
}
