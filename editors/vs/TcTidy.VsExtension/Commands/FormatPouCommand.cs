using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using Community.VisualStudio.Toolkit;
using EnvDTE;
using Microsoft.VisualStudio.Shell;
using TcTidy.VsExtension.Core;
using Task = System.Threading.Tasks.Task;

namespace TcTidy.VsExtension
{
    /// <summary>
    /// The "Format with TcTidy" command launched from the Solution Explorer (a PLC source object, folder
    /// or project). Each selected TwinCAT source file is formatted as a WHOLE POU, its body plus every
    /// method/action/property: files OPEN in the editor go in place through the TwinCAT Automation
    /// Interface (no reload prompt), rooted at the selected POU's tree item and recursed; every other file
    /// is formatted on disk in a single tctidy batch (already whole-POU). Folders and PLC projects recurse.
    /// Launching from inside one element's editor is handled by <see cref="FormatElementCommand"/>.
    /// <para>
    /// In test mode (the TcTidy menu's Performance Report toggle), it records a precise end-to-end
    /// timing breakdown, file detection, open-doc detection, the API and disk format phases, total,
    /// via <see cref="PerfMode"/>. Off by default, so normal use is unaffected.
    /// </para>
    /// </summary>
    [Command(PackageGuids.guidTcTidyCmdSetString, PackageIds.FormatPouCommandId)]
    internal sealed class FormatPouCommand : BaseCommand<FormatPouCommand>
    {
        protected override async Task ExecuteAsync(OleMenuCmdEventArgs e)
        {
            // Test mode only: capture the wall-clock of each phase. `total` spans the whole command;
            // `sw` is reused per phase.
            var perf = PerfMode.Enabled ? new PerfReport() : null;
            var total = Stopwatch.StartNew();
            var sw = Stopwatch.StartNew();

            var exe = FormatSupport.LocateExe();
            perf?.Add("Resolve tctidy.exe", sw.Elapsed);
            if (exe == null)
            {
                await FormatSupport.ShowExeMissingAsync();
                return;
            }

            await ThreadHelper.JoinableTaskFactory.SwitchToMainThreadAsync();
            var dte = await VS.GetRequiredServiceAsync<DTE, DTE>();

            // The recursive walk of the selected folder/project, "detecting all the files".
            sw.Restart();
            var targets = SourceFiles.Resolve(dte);
            perf?.Add("Detect files", sw.Elapsed);
            if (targets.Count == 0)
            {
                await VS.MessageBox.ShowAsync(
                    "TcTidy", "Select a TwinCAT POU, folder or PLC project to format.");
                return;
            }

            // Split the targets: files open in the editor are formatted in memory via the Automation
            // Interface (no reload prompt), rooted at the selected POU's tree item and recursed into every
            // method/action/property; the rest are formatted on disk. Each open POU is flushed first (a
            // targeted IDE save, only when dirty) so the on-screen text, including unsaved edits, is what
            // gets read and formatted. Closed files have no in-memory edits to flush.
            sw.Restart();
            var openDocs = OpenDocuments.Resolve(dte);
            var apiItems = new List<OpenItem>();
            var diskTargets = new List<string>();
            foreach (var target in targets)
            {
                if (openDocs.TryGetValue(target.Key, out var doc) && TryGetTreeItem(target.Value, out var node))
                    apiItems.Add(new OpenItem(
                        target.Value, node, doc, dte, recurse: true,
                        SourceFiles.QualifiedName(target.Value), filePath: target.Key));
                else
                    diskTargets.Add(target.Key);
            }
            perf?.Add("Detect open docs", sw.Elapsed);

            var runner = new TctidyRunner(
                exe, TimeSpan.FromSeconds(60 + targets.Count * 0.2), FormatSupport.SolutionDirectory(dte));

            // Open docs go first, in memory via the Automation Interface (UI thread, COM access); the
            // closed files then go to the single-spawn disk batch, off the UI thread. The Core run shows
            // the status-bar feedback, turns a phase that throws into failures of that phase's targets
            // (keeping the other phase's results), always stops the spinner, and returns the one result
            // reported below.
            var result = await ProjectFormatRun.ExecuteAsync(
                FormatSupport.RunProgress,
                apiItems.Select(item => item.Name).ToList(),
                () => OpenDocFormatter.Format(dte, apiItems, runner),
                diskTargets,
                () => Task.Run(() => new DiskBatchStrategy(runner).FormatAll(diskTargets)));

            perf?.AddFormat("Format", result.OpenElapsed + result.DiskElapsed, targets.Count, result.Changed, result.Unchanged, indented: false);
            perf?.AddFormat("Open via API", result.OpenElapsed, apiItems.Count, result.Open.Changed, result.Open.Unchanged, indented: true);
            perf?.AddFormat("Disk batch", result.DiskElapsed, diskTargets.Count, result.Disk.Changed, result.Disk.Unchanged, indented: true);

            await FormatSupport.ReportAsync(result.Report);

            if (perf != null)
            {
                total.Stop();
                perf.Add("Total (end-to-end)", total.Elapsed);
                perf.Files = targets.Count;
                perf.Changed = result.Changed;
                perf.Unchanged = result.Unchanged;
                perf.Failed = result.Failures.Count;
                await PerfMode.ReportAsync(perf);
            }
        }

        // The selected item's COM tree item roots the recursion. Falls back to the disk path if it can't
        // be read, rather than formatting from the wrong node.
        private static bool TryGetTreeItem(ProjectItem item, out object node)
        {
            ThreadHelper.ThrowIfNotOnUIThread();
            try
            {
                node = item.Object;
                return node != null;
            }
            catch
            {
                node = null!;
                return false;
            }
        }
    }
}
