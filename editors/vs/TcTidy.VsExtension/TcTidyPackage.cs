using System;
using System.Runtime.InteropServices;
using System.Threading;
using Community.VisualStudio.Toolkit;
using Microsoft.VisualStudio;
using Microsoft.VisualStudio.Shell;
using Task = System.Threading.Tasks.Task;

namespace TcTidy.VsExtension
{
    /// <summary>
    /// The TcTidy VS package. Loads in the background and registers the commands defined in
    /// <c>VSCommandTable.vsct</c>.
    /// </summary>
    [PackageRegistration(UseManagedResourcesOnly = true, AllowsBackgroundLoading = true)]
    [InstalledProductRegistration("TcTidy for TwinCAT", "Format TwinCAT Structured Text with tctidy.", "0.4.1")]
    [ProvideMenuResource("Menus.ctmenu", 1)]
    // Load as soon as a solution is open so the command is registered and live on the
    // TwinCAT context menus (a TwinCAT project is always a fully-loaded solution).
    [ProvideAutoLoad(VSConstants.UICONTEXT.SolutionExistsAndFullyLoaded_string, PackageAutoLoadFlags.BackgroundLoad)]
    // Also load when the shell starts without a solution: the update check then runs at startup,
    // and the menu's toggles show their saved state (an unloaded package leaves them unchecked).
    [ProvideAutoLoad(VSConstants.UICONTEXT.NoSolution_string, PackageAutoLoadFlags.BackgroundLoad)]
    [Guid(PackageGuids.guidTcTidyPackageString)]
    public sealed class TcTidyPackage : ToolkitPackage
    {
        protected override async Task InitializeAsync(
            CancellationToken cancellationToken,
            IProgress<ServiceProgressData> progress)
        {
            await this.RegisterCommandsAsync();

            // Off the load path: at most one small download a day, silent unless there is news.
            UpdateNotifier.CheckOnLoadAsync(DisposalToken).FireAndForget();

            // Background load resumes the method off the UI thread, and SolutionEvents' constructor
            // throws there, failing the whole package load.
            await JoinableTaskFactory.SwitchToMainThreadAsync(cancellationToken);

            // The settings page edits the open solution's .tctidy.toml: stop its server with it.
            VS.Events.SolutionEvents.OnAfterCloseSolution += FormattingSettingsCommand.Pages.Stop;
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing)
            {
                VS.Events.SolutionEvents.OnAfterCloseSolution -= FormattingSettingsCommand.Pages.Stop;
                FormattingSettingsCommand.Pages.Dispose();
            }
            base.Dispose(disposing);
        }
    }
}
