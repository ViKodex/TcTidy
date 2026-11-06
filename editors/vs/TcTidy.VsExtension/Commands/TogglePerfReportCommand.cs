using System;
using Community.VisualStudio.Toolkit;
using Microsoft.VisualStudio.Shell;
using TcTidy.VsExtension.Core;
using Task = System.Threading.Tasks.Task;

namespace TcTidy.VsExtension
{
    /// <summary>
    /// The TcTidy menu's "Performance Report" toggle: flips and persists
    /// <see cref="GeneralOptions.PerfReportEnabled"/>, the gate <see cref="PerfMode"/> reads.
    /// The checkmark reflects the persisted state; each flip is confirmed in the status bar
    /// (the project's convention: status bar for outcomes, never a dialog for a success path).
    /// Takes effect on the next Format run, no shell restart needed.
    /// </summary>
    [Command(PackageGuids.guidTcTidyCmdSetString, PackageIds.TogglePerfReportCommandId)]
    internal sealed class TogglePerfReportCommand : BaseCommand<TogglePerfReportCommand>
    {
        protected override void BeforeQueryStatus(EventArgs e) =>
            Command.Checked = GeneralOptions.Instance.PerfReportEnabled;

        protected override async Task ExecuteAsync(OleMenuCmdEventArgs e)
        {
            var options = await GeneralOptions.GetLiveInstanceAsync();
            options.PerfReportEnabled = !options.PerfReportEnabled;
            await options.SaveAsync();

            await VS.StatusBar.ShowMessageAsync(PerfToggle.StatusMessage(options.PerfReportEnabled));
        }
    }
}
