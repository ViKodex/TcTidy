using System;
using System.Collections.Generic;
using System.IO;
using Community.VisualStudio.Toolkit;
using EnvDTE;
using Microsoft.VisualStudio.Shell;
using Microsoft.VisualStudio.Shell.Interop;
using TcTidy.VsExtension.Core;
using Task = System.Threading.Tasks.Task;

namespace TcTidy.VsExtension
{
    /// <summary>
    /// Bits shared by the two "Format with TcTidy" commands, the tree-launched
    /// <see cref="FormatPouCommand"/> (whole POU, recursive) and the editor-launched
    /// <see cref="FormatElementCommand"/> (the open element alone): locating <c>tctidy.exe</c> and
    /// reporting a finished run.
    /// </summary>
    internal static class FormatSupport
    {
        /// <summary>
        /// Locate <c>tctidy.exe</c> next to the extension (installer payload) with a <c>PATH</c> fallback,
        /// via the tested <see cref="TctidyLocator"/>. Returns null when it cannot be found.
        /// </summary>
        public static string? LocateExe() =>
            TctidyLocator.Resolve(
                Path.GetDirectoryName(typeof(FormatSupport).Assembly.Location) ?? string.Empty,
                (Environment.GetEnvironmentVariable("PATH") ?? string.Empty).Split(Path.PathSeparator));

        /// <summary>
        /// The open solution's directory, where its <c>.tctidy.toml</c> belongs; null when no saved
        /// solution is open. UI thread only.
        /// </summary>
        public static string? SolutionDirectory(DTE dte)
        {
            ThreadHelper.ThrowIfNotOnUIThread();
            try { return Core.SolutionDirectory.Of(dte.Solution?.FullName); }
            catch { return null; }
        }

        public static async Task ShowExeMissingAsync() =>
            await VS.MessageBox.ShowErrorAsync(
                "TcTidy", "tctidy.exe was not found next to the extension or on PATH.");

        /// <summary>
        /// Report a finished run: write the per-file breakdown to the "TcTidy" Output pane, each failure
        /// (left untouched) on its own line with its full path or qualified element name and tctidy's
        /// diagnostic, and show the one-line summary in the status bar. When something failed, pop a
        /// short warning dialog so a partial failure can't be missed, then bring the Output pane to the
        /// front so its per-file details are right there once the dialog is dismissed.
        /// <para>
        /// Activating the pane *after* the modal is deliberate: a pane activation issued before a modal
        /// does not paint until the modal's own message loop ends, so the Output window would stay behind a
        /// sibling docked tab (e.g. the Error List) the whole time the dialog is up. A fully clean run
        /// stays silent.
        /// </para>
        /// </summary>
        public static Task ReportAsync(
            int changed, int unchanged, IReadOnlyList<FileFailure> failures, int apiCount, int diskCount) =>
            ReportAsync(new FormatReport(changed, unchanged, failures, apiCount, diskCount));

        /// <inheritdoc cref="ReportAsync(int, int, IReadOnlyList{FileFailure}, int, int)"/>
        public static async Task ReportAsync(FormatReport report)
        {
            await TcTidyOutput.WriteLineAsync(report.Format());
            await VS.StatusBar.ShowMessageAsync(report.Summary);

            if (report.FailureCount > 0)
            {
                await VS.MessageBox.ShowAsync(
                    "TcTidy", report.FailureNotice,
                    OLEMSGICON.OLEMSGICON_WARNING, OLEMSGBUTTON.OLEMSGBUTTON_OK);
                await TcTidyOutput.ActivateAsync();
            }
        }

        /// <summary>The status-bar feedback of a project-wide run, driven by <see cref="ProjectFormatRun"/>:
        /// a spinner plus a message naming the current phase.</summary>
        public static IRunProgress RunProgress { get; } = new StatusBarProgress();

        /// <summary>Start the status-bar spinner and announce a project-wide run is in flight, so a large
        /// run (hundreds of POUs) is not silent while it works. Paired with <see cref="EndWorkingAsync"/>.</summary>
        public static async Task ShowWorkingAsync(int total)
        {
            await VS.StatusBar.StartAnimationAsync(StatusAnimation.Sync);
            await VS.StatusBar.ShowMessageAsync($"TcTidy: formatting {total} file(s)…");
        }

        /// <summary>Update the in-flight status-bar message to name the current phase.</summary>
        public static async Task ShowPhaseAsync(string phase) =>
            await VS.StatusBar.ShowMessageAsync($"TcTidy: {phase}");

        /// <summary>Stop the status-bar spinner. Best-effort: a stuck spinner would be a visible bug, but
        /// stopping it must never itself throw out of the command's finally block.</summary>
        public static async Task EndWorkingAsync()
        {
            try { await VS.StatusBar.EndAnimationAsync(StatusAnimation.Sync); }
            catch { /* best effort, never let stopping the spinner break the run */ }
        }

        private sealed class StatusBarProgress : IRunProgress
        {
            public Task StartAsync(int total) => ShowWorkingAsync(total);

            public Task OpenPhaseAsync(int count) => ShowPhaseAsync($"formatting {count} open POU(s)…");

            // Shown for the whole disk batch, which reports no progress of its own.
            public Task DiskPhaseAsync(int count) => ShowPhaseAsync($"formatting {count} file(s) on disk…");

            public Task EndAsync() => EndWorkingAsync();
        }
    }
}
