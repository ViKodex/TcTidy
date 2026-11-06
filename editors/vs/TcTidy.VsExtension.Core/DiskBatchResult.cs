using System;
using System.Collections.Generic;
using System.Linq;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// Turns one <c>tctidy --files-from -</c> run into a <see cref="BatchOutcome"/>, from whether each
    /// file changed and how the run ended. A changed file was formatted, whatever the exit code:
    /// tctidy writes a file atomically and only when it changes it. Exit 0 leaves every other file
    /// unchanged, and exit 3 fails exactly the files tctidy named. Any other ending (tctidy could
    /// not start, timed out, stopped on a usage or configuration error, or crashed) leaves the
    /// other files unconfirmed, so each fails with tctidy's diagnostic when it named the file and
    /// with the run's error otherwise: no file is counted unchanged on a run that did not finish.
    /// Pure, so every ending is unit-testable without tctidy.
    /// </summary>
    public static class DiskBatchResult
    {
        /// <param name="files">Each file handed to tctidy and still readable afterwards, in order.</param>
        /// <param name="run">How tctidy ended.</param>
        /// <param name="windowsPaths">Compare paths as Windows does (see <see cref="BatchDiagnostics"/>).</param>
        public static BatchOutcome Build(
            IReadOnlyList<(string Path, bool Changed)> files, EngineRun run, bool windowsPaths)
        {
            if (files == null) throw new ArgumentNullException(nameof(files));
            if (run == null) throw new ArgumentNullException(nameof(run));

            var diagnostics = BatchDiagnostics.Parse(run.Stderr, files.Select(f => f.Path), windowsPaths);
            var runError = RunError(run, diagnostics);

            var (changed, unchanged) = (0, 0);
            var failures = new List<FileFailure>();
            foreach (var (path, isChanged) in files)
            {
                if (isChanged)
                {
                    changed++;
                }
                else if (run.ExitCode != 0 && diagnostics.TryGetMessage(path, out var message))
                {
                    failures.Add(new FileFailure(path, message));
                }
                else if (runError != null)
                {
                    failures.Add(new FileFailure(path, runError));
                }
                else
                {
                    unchanged++;
                }
            }

            return new BatchOutcome(changed, unchanged, failures);
        }

        // Why a file tctidy neither changed nor named cannot count as unchanged; null when it can,
        // that is when tctidy went through the whole list (exit 0, or 3 for the files it named).
        private static string? RunError(EngineRun run, BatchDiagnostics diagnostics)
        {
            if (run.Error != null) return run.Error;
            if (run.ExitCode == 0 || run.ExitCode == 3) return null;

            var reason = $"tctidy failed (exit code {run.ExitCode})";
            return diagnostics.OtherLines.Count == 0
                ? reason
                : reason + ": " + string.Join("; ", diagnostics.OtherLines);
        }
    }
}
