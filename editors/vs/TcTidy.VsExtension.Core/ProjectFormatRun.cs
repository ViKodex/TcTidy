using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Reflection;
using System.Threading.Tasks;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// The project-wide "Format with TcTidy" run: the open POUs first, in place, then the closed
    /// files on disk in one batch. A phase that throws (tctidy could not start, a COM call failed)
    /// fails each of its own targets with the reason instead of ending the run, so the results of
    /// the other phase are kept. The progress feedback is stopped exactly once, after both phases,
    /// and the run always returns the one result the caller reports. Pure, so these guarantees are
    /// unit-tested without Visual Studio.
    /// <para>
    /// Every await resumes on the caller's context (no <c>ConfigureAwait(false)</c>): the open phase
    /// touches COM and must run on the UI thread the command is on.
    /// </para>
    /// </summary>
    public static class ProjectFormatRun
    {
        /// <param name="progress">The in-flight feedback; a failure to update it is ignored.</param>
        /// <param name="openPous">The open POUs' names, which name their failures.</param>
        /// <param name="formatOpen">Formats the open POUs; called only when there are some.</param>
        /// <param name="diskFiles">The closed files' paths, which name their failures.</param>
        /// <param name="formatDisk">Formats the closed files; called only when there are some.</param>
        public static async Task<ProjectFormatResult> ExecuteAsync(
            IRunProgress progress,
            IReadOnlyList<string> openPous, Func<BatchOutcome> formatOpen,
            IReadOnlyList<string> diskFiles, Func<Task<BatchOutcome>> formatDisk)
        {
            if (progress == null) throw new ArgumentNullException(nameof(progress));
            if (openPous == null) throw new ArgumentNullException(nameof(openPous));
            if (formatOpen == null) throw new ArgumentNullException(nameof(formatOpen));
            if (diskFiles == null) throw new ArgumentNullException(nameof(diskFiles));
            if (formatDisk == null) throw new ArgumentNullException(nameof(formatDisk));

            var open = Empty();
            var disk = Empty();
            var openElapsed = TimeSpan.Zero;
            var diskElapsed = TimeSpan.Zero;

            await QuietlyAsync(() => progress.StartAsync(openPous.Count + diskFiles.Count));
            try
            {
                if (openPous.Count > 0)
                {
                    await QuietlyAsync(() => progress.OpenPhaseAsync(openPous.Count));
                    var sw = Stopwatch.StartNew();
                    try { open = formatOpen(); }
                    catch (Exception ex) { open = Failed(openPous, "open POU formatting", ex); }
                    openElapsed = sw.Elapsed;
                }

                if (diskFiles.Count > 0)
                {
                    await QuietlyAsync(() => progress.DiskPhaseAsync(diskFiles.Count));
                    var sw = Stopwatch.StartNew();
                    try { disk = await formatDisk(); }
                    catch (Exception ex) { disk = Failed(diskFiles, "disk batch", ex); }
                    diskElapsed = sw.Elapsed;
                }
            }
            finally
            {
                await QuietlyAsync(progress.EndAsync);
            }

            return new ProjectFormatResult(
                open, openPous.Count, openElapsed, disk, diskFiles.Count, diskElapsed);
        }

        private static BatchOutcome Empty() => new BatchOutcome(0, 0, Array.Empty<FileFailure>());

        // A phase that threw reported nothing per target, so each of its targets fails with the reason.
        private static BatchOutcome Failed(IReadOnlyList<string> targets, string phase, Exception error)
        {
            while ((error is AggregateException || error is TargetInvocationException)
                && error.InnerException != null)
            {
                error = error.InnerException;
            }

            var failures = new List<FileFailure>(targets.Count);
            foreach (var target in targets)
                failures.Add(new FileFailure(target, $"{phase} failed: {error.Message}"));
            return new BatchOutcome(0, 0, failures);
        }

        // Feedback is cosmetic: failing to show it must not stop the run or lose its report.
        private static async Task QuietlyAsync(Func<Task> update)
        {
            try { await update(); }
            catch (Exception) { /* best effort */ }
        }
    }
}
