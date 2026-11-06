using System;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// tctidy refused or failed to format. By tctidy's non-destruction guarantee, the
    /// original file or text is left untouched when this is thrown.
    /// </summary>
    public sealed class FormatterException : Exception
    {
        /// <summary>The process exit code (3 = parse/format error, 2 = usage, -1 = timeout/spawn).</summary>
        public int ExitCode { get; }

        public FormatterException(int exitCode, string message) : base(message)
            => ExitCode = exitCode;
    }
}
