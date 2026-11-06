using System.Collections.Generic;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// Runs the <c>tctidy</c> binary. A pure wrapper with no Visual Studio dependency,
    /// so it can be unit-tested without an IDE or a TwinCAT install.
    /// </summary>
    public interface ITctidyRunner
    {
        /// <summary>
        /// Format raw ST text via stdin/stdout (<c>tctidy - [--decl|--impl]</c>) and return
        /// the formatted text. Throws <see cref="FormatterException"/> (ExitCode 3) on a
        /// parse/format error, in which case the caller keeps the original text.
        /// </summary>
        /// <param name="sourcePath">
        /// The file the text comes from, when known. tctidy then formats it with the
        /// <c>.tctidy.toml</c> that applies to that file (<c>--stdin-filepath</c>), exactly as it
        /// would format the file on disk.
        /// </param>
        string FormatText(string source, StSection section, string? sourcePath = null);

        /// <summary>
        /// Format a whole TwinCAT XML document (e.g. the XML the Automation Interface's
        /// <c>ProduceXml</c> returns) via stdin/stdout (<c>tctidy -</c>), reformatting the ST
        /// inside while preserving every byte outside it, and return the formatted document.
        /// Throws <see cref="FormatterException"/> (ExitCode 3) on a parse/format error, in
        /// which case the caller keeps the original document.
        /// </summary>
        string FormatXml(string xml);

        /// <summary>
        /// Format a TwinCAT file in place (<c>tctidy &lt;path&gt;</c>), preserving every byte
        /// of the XML outside the ST. Returns whether the file changed. Throws
        /// <see cref="FormatterException"/> (ExitCode 3) on a parse/format error, in which
        /// case the file is left untouched.
        /// </summary>
        FormatResult FormatFile(string path);

        /// <summary>
        /// Format several TwinCAT files in a single tctidy invocation (the path list is
        /// passed on stdin via <c>tctidy --files-from -</c>), amortizing process startup
        /// across the whole batch. Files are formatted in place; a per-file parse error
        /// leaves that file untouched and does not abort the batch. Returns the aggregate
        /// <see cref="BatchOutcome"/> (changed/unchanged counts derived by comparing each
        /// file before and after, failures gathered from unreadable files and the stderr
        /// lines naming each exact path). A run that does not finish (tctidy could not be
        /// started, timed out, stopped on a usage or configuration error, or crashed) does
        /// not throw: the files it already rewrote count as changed, and every other file
        /// fails with the reason (see <see cref="DiskBatchResult"/>).
        /// </summary>
        BatchOutcome FormatFiles(IReadOnlyList<string> paths);
    }
}
