using System;
using System.Collections.Generic;
using Community.VisualStudio.Toolkit;
using EnvDTE;
using Microsoft.VisualStudio.Shell;
using TcTidy.VsExtension.Core;
using Task = System.Threading.Tasks.Task;

namespace TcTidy.VsExtension
{
    /// <summary>
    /// The "Format with TcTidy" command launched from inside a PLC code editor. It formats exactly the
    /// element on screen (the POU body, or a single method/action/property accessor) and nothing else:
    /// "you format what you clicked on". It never recurses into siblings, but always formats both the
    /// element's declaration and its implementation, even when only one pane was focused.
    /// <para>
    /// The active window's tree item is read and rewritten through the TwinCAT Automation Interface, so
    /// the edit lands in place with no reload prompt; the buffer is flushed first so unsaved on-screen
    /// edits are what get formatted. This is the editor counterpart to <see cref="FormatPouCommand"/>,
    /// which formats whole POUs selected in the Solution Explorer.
    /// </para>
    /// </summary>
    [Command(PackageGuids.guidTcTidyCmdSetString, PackageIds.FormatElementCommandId)]
    internal sealed class FormatElementCommand : BaseCommand<FormatElementCommand>
    {
        protected override async Task ExecuteAsync(OleMenuCmdEventArgs e)
        {
            var exe = FormatSupport.LocateExe();
            if (exe == null)
            {
                await FormatSupport.ShowExeMissingAsync();
                return;
            }

            await ThreadHelper.JoinableTaskFactory.SwitchToMainThreadAsync();
            var dte = await VS.GetRequiredServiceAsync<DTE, DTE>();

            if (!TryGetActiveElement(dte, out var item, out var document, out var treeItem))
            {
                await VS.MessageBox.ShowAsync(
                    "TcTidy", "Open a TwinCAT POU, method or action in the editor to format.");
                return;
            }

            var runner = new TctidyRunner(exe, TimeSpan.FromSeconds(60), FormatSupport.SolutionDirectory(dte));
            var openItem = new OpenItem(
                item, treeItem, document, dte, recurse: false, SourceFiles.QualifiedName(item),
                SourceFiles.FilePath(item));
            var outcome = OpenDocFormatter.Format(dte, new List<OpenItem> { openItem }, runner);

            await FormatSupport.ReportAsync(
                outcome.Changed, outcome.Unchanged, outcome.Failures, apiCount: 1, diskCount: 0);
        }

        // The active editor window's project item is the focused element (POU body, method, action, or
        // property accessor); its .Object is the tree item to format and its document backs the flush.
        private static bool TryGetActiveElement(
            DTE dte, out ProjectItem item, out Document document, out object treeItem)
        {
            ThreadHelper.ThrowIfNotOnUIThread();
            item = null!;
            document = null!;
            treeItem = null!;
            try
            {
                var window = dte.ActiveWindow;
                item = window?.ProjectItem!;
                document = window?.Document!;
                if (item == null || document == null) return false;
                treeItem = item.Object;
                return treeItem != null;
            }
            catch
            {
                return false;
            }
        }
    }
}
