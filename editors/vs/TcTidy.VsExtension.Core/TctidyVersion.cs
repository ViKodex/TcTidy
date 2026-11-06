using System;
using System.Diagnostics;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// Reads the engine version by spawning <c>tctidy --version</c> (clap prints
    /// <c>tctidy X.Y.Z</c>). Best-effort by design: it feeds the About dialog, so any
    /// failure, missing exe, timeout, unexpected output, is a null, never an exception.
    /// </summary>
    public static class TctidyVersion
    {
        private const int TimeoutMs = 5_000;
        private static readonly char[] SemVerSuffixStart = { '-', '+' };

        public static string? Read(string exePath)
        {
            try
            {
                var psi = new ProcessStartInfo
                {
                    FileName = exePath,
                    Arguments = "--version",
                    UseShellExecute = false,
                    CreateNoWindow = true,
                    RedirectStandardOutput = true,
                    RedirectStandardError = true,
                };

                using var process = Process.Start(psi);
                if (process == null) return null;

                var stdout = process.StandardOutput.ReadToEnd();
                if (!process.WaitForExit(TimeoutMs))
                {
                    try { process.Kill(); } catch { /* already gone */ }
                    return null;
                }
                if (process.ExitCode != 0) return null;

                return Parse(stdout);
            }
            catch
            {
                return null;
            }
        }

        /// <summary>
        /// "tctidy 0.3.0" → "0.3.0"; anything that is not a version is a null. A SemVer prerelease or
        /// build suffix ("0.4.0-rc.1", "0.4.0+abc") is kept for display: only the numeric core before
        /// it has to parse as a <see cref="Version"/>.
        /// </summary>
        public static string? Parse(string output)
        {
            var parts = output.Trim().Split(' ');
            var candidate = parts[parts.Length - 1];
            var suffix = candidate.IndexOfAny(SemVerSuffixStart);
            if (suffix == candidate.Length - 1) return null; // a dangling "-" or "+" is not SemVer
            var core = suffix < 0 ? candidate : candidate.Substring(0, suffix);
            return Version.TryParse(core, out _) ? candidate : null;
        }
    }
}
