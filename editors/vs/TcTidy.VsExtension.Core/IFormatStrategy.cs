using System.Collections.Generic;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// A path-based way to format a batch of TwinCAT files. The live command uses
    /// <see cref="DiskBatchStrategy"/> for closed files; open tree items use
    /// <see cref="OpenPouFormatter"/> through the VS shell instead of this seam.
    /// <see cref="DiskPerFileStrategy"/> remains available for comparison.
    /// </summary>
    public interface IFormatStrategy
    {
        /// <summary>A short, stable identifier for reports.</summary>
        string Name { get; }

        /// <summary>Format every target, returning an aggregate outcome.</summary>
        BatchOutcome FormatAll(IReadOnlyList<string> targets);
    }
}
