using System;
using System.Collections.Generic;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// Spawns <c>tctidy</c> once for the whole batch (the path list is passed on stdin via
    /// <c>tctidy --files-from -</c>), paying process startup a single time. The runner
    /// reconstructs the changed/unchanged/failed breakdown by comparing each file before
    /// and after, so the amortized spawn cost comes at no loss of reporting.
    /// </summary>
    public sealed class DiskBatchStrategy : IFormatStrategy
    {
        private readonly ITctidyRunner _runner;

        public DiskBatchStrategy(ITctidyRunner runner)
        {
            _runner = runner ?? throw new ArgumentNullException(nameof(runner));
        }

        public string Name => "disk-batch";

        public BatchOutcome FormatAll(IReadOnlyList<string> targets) => _runner.FormatFiles(targets);
    }
}
