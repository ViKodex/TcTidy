using System;
using System.Collections.Generic;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// Spawns <c>tctidy</c> once per file. Each file's parse error is caught so it does not
    /// abort the batch. The per-file process startup is paid N times: this is the baseline the
    /// spawn-once <see cref="DiskBatchStrategy"/> (the one the Format command uses) is benchmarked
    /// against.
    /// </summary>
    public sealed class DiskPerFileStrategy : IFormatStrategy
    {
        private readonly ITctidyRunner _runner;

        public DiskPerFileStrategy(ITctidyRunner runner)
        {
            _runner = runner ?? throw new ArgumentNullException(nameof(runner));
        }

        public string Name => "disk-per-file";

        public BatchOutcome FormatAll(IReadOnlyList<string> targets)
        {
            var (changed, unchanged) = (0, 0);
            var failures = new List<FileFailure>();
            foreach (var path in targets)
            {
                try
                {
                    if (_runner.FormatFile(path).Changed) changed++;
                    else unchanged++;
                }
                catch (FormatterException ex)
                {
                    failures.Add(new FileFailure(path, ex.Message));
                }
            }
            return new BatchOutcome(changed, unchanged, failures);
        }
    }
}
