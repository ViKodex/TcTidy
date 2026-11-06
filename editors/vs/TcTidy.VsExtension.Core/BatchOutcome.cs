using System.Collections.Generic;

namespace TcTidy.VsExtension.Core
{
    /// <summary>Aggregate result of formatting a batch of files through an
    /// <see cref="IFormatStrategy"/>.</summary>
    public sealed class BatchOutcome
    {
        /// <summary>Files whose bytes changed. Zero when the strategy cannot report it
        /// per file (e.g. the single-invocation disk batch).</summary>
        public int Changed { get; }

        /// <summary>Files left unchanged. Zero when not reported per file.</summary>
        public int Unchanged { get; }

        /// <summary>Per-file failures (each left untouched); empty on full success.</summary>
        public IReadOnlyList<FileFailure> Failures { get; }

        public BatchOutcome(int changed, int unchanged, IReadOnlyList<FileFailure> failures)
        {
            Changed = changed;
            Unchanged = unchanged;
            Failures = failures;
        }
    }
}
