using System;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// How one tctidy process ended: it exited with a code and its stderr, or it never produced an
    /// exit code worth reading (it could not be started, timed out, or its pipes failed).
    /// </summary>
    public sealed class EngineRun
    {
        private EngineRun(int? exitCode, string stderr, string? error)
        {
            ExitCode = exitCode;
            Stderr = stderr;
            Error = error;
        }

        /// <summary>The process exit code; null when <see cref="Error"/> says why there is none.</summary>
        public int? ExitCode { get; }

        /// <summary>Everything tctidy wrote to stderr; empty when it did not exit.</summary>
        public string Stderr { get; }

        /// <summary>Why tctidy did not exit normally (spawn failure, timeout); null when it did.</summary>
        public string? Error { get; }

        public static EngineRun Exited(int exitCode, string stderr) =>
            new EngineRun(exitCode, stderr ?? throw new ArgumentNullException(nameof(stderr)), null);

        public static EngineRun Aborted(string error) =>
            new EngineRun(null, string.Empty, error ?? throw new ArgumentNullException(nameof(error)));
    }
}
