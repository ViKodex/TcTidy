using System;
using System.Collections.Generic;
using System.Linq;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// What a <see cref="ProjectFormatRun"/> did: the outcome and duration of each phase, and the
    /// single <see cref="FormatReport"/> that sums them up.
    /// </summary>
    public sealed class ProjectFormatResult
    {
        public ProjectFormatResult(
            BatchOutcome open, int openCount, TimeSpan openElapsed,
            BatchOutcome disk, int diskCount, TimeSpan diskElapsed)
        {
            Open = open ?? throw new ArgumentNullException(nameof(open));
            Disk = disk ?? throw new ArgumentNullException(nameof(disk));
            OpenCount = openCount;
            DiskCount = diskCount;
            OpenElapsed = openElapsed;
            DiskElapsed = diskElapsed;
        }

        /// <summary>The open POUs formatted in place.</summary>
        public BatchOutcome Open { get; }

        /// <summary>The closed files formatted on disk.</summary>
        public BatchOutcome Disk { get; }

        public int OpenCount { get; }
        public int DiskCount { get; }
        public TimeSpan OpenElapsed { get; }
        public TimeSpan DiskElapsed { get; }

        public int Changed => Open.Changed + Disk.Changed;
        public int Unchanged => Open.Unchanged + Disk.Unchanged;

        /// <summary>Every failure, open POUs first.</summary>
        public IReadOnlyList<FileFailure> Failures => Open.Failures.Concat(Disk.Failures).ToList();

        /// <summary>The run's one summary, covering both phases.</summary>
        public FormatReport Report => new FormatReport(Changed, Unchanged, Failures, OpenCount, DiskCount);
    }
}
