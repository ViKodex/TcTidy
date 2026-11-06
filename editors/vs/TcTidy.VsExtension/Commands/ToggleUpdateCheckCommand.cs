using System;
using Community.VisualStudio.Toolkit;
using Microsoft.VisualStudio.Shell;
using TcTidy.VsExtension.Core;
using Task = System.Threading.Tasks.Task;

namespace TcTidy.VsExtension
{
    /// <summary>
    /// The TcTidy menu's "Check for Updates Automatically" toggle: flips and persists
    /// <see cref="GeneralOptions.UpdateCheckEnabled"/>, the gate of the check at package load
    /// (<see cref="UpdateNotifier"/>). Greyed out when the administrator policy turns checks off.
    /// </summary>
    [Command(PackageGuids.guidTcTidyCmdSetString, PackageIds.ToggleUpdateCheckCommandId)]
    internal sealed class ToggleUpdateCheckCommand : BaseCommand<ToggleUpdateCheckCommand>
    {
        protected override void BeforeQueryStatus(EventArgs e)
        {
            var allowed = !UpdateNotifier.DisabledByPolicy;
            Command.Enabled = allowed;
            Command.Checked = allowed && GeneralOptions.Instance.UpdateCheckEnabled;
        }

        protected override async Task ExecuteAsync(OleMenuCmdEventArgs e)
        {
            var options = await GeneralOptions.GetLiveInstanceAsync();
            options.UpdateCheckEnabled = !options.UpdateCheckEnabled;
            await options.SaveAsync();

            await VS.StatusBar.ShowMessageAsync(UpdateText.ToggleStatus(options.UpdateCheckEnabled));
        }
    }
}
