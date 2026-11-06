using System;
using System.Diagnostics;
using System.IO;
using System.Reflection;
using Community.VisualStudio.Toolkit;
using Microsoft.VisualStudio.Shell;
using Microsoft.VisualStudio.Shell.Interop;
using TcTidy.VsExtension.Core;
using Task = System.Threading.Tasks.Task;

namespace TcTidy.VsExtension
{
    /// <summary>
    /// The TcTidy top-level menu's entries: Documentation, Report a Bug, Formatting Settings, Check for
    /// Updates, About.
    /// Thin shells over the tested Core pieces (<see cref="ProjectLinks"/>, <see cref="AboutText"/>,
    /// <see cref="TctidyVersion"/>, <see cref="SettingsPageHost"/>); all they add is the browser
    /// launch and the message box.
    /// </summary>
    [Command(PackageGuids.guidTcTidyCmdSetString, PackageIds.DocumentationCommandId)]
    internal sealed class DocumentationCommand : BaseCommand<DocumentationCommand>
    {
        protected override async Task ExecuteAsync(OleMenuCmdEventArgs e) =>
            await Browser.OpenAsync(ProjectLinks.Documentation);
    }

    [Command(PackageGuids.guidTcTidyCmdSetString, PackageIds.ReportBugCommandId)]
    internal sealed class ReportBugCommand : BaseCommand<ReportBugCommand>
    {
        protected override async Task ExecuteAsync(OleMenuCmdEventArgs e) =>
            await Browser.OpenAsync(ProjectLinks.NewBugReport(ExtensionVersion.Current));
    }

    /// <summary>
    /// Opens the formatting settings page for the open solution's <c>.tctidy.toml</c> in the default
    /// browser, like Report a Bug: tctidy serves the page on loopback (<c>tctidy --settings</c>) with
    /// a Before/After preview of every setting. One server per IDE, reused while it runs and stopped
    /// when the solution closes or the IDE exits.
    /// </summary>
    [Command(PackageGuids.guidTcTidyCmdSetString, PackageIds.FormattingSettingsCommandId)]
    internal sealed class FormattingSettingsCommand : BaseCommand<FormattingSettingsCommand>
    {
        internal static readonly SettingsPageHost Pages = new SettingsPageHost(
            (exe, root) => SettingsServer.Start(exe, root, TimeSpan.FromSeconds(15)));

        protected override async Task ExecuteAsync(OleMenuCmdEventArgs e)
        {
            await ThreadHelper.JoinableTaskFactory.SwitchToMainThreadAsync();
            var dte = await VS.GetRequiredServiceAsync<EnvDTE.DTE, EnvDTE.DTE>();
            var root = FormatSupport.SolutionDirectory(dte);
            if (root == null)
            {
                await VS.MessageBox.ShowAsync(
                    "TcTidy",
                    "Open a solution first: its formatting settings are saved next to it, in .tctidy.toml.");
                return;
            }

            var exe = FormatSupport.LocateExe();
            if (exe == null)
            {
                await FormatSupport.ShowExeMissingAsync();
                return;
            }

            string url;
            try
            {
                await VS.StatusBar.ShowMessageAsync("TcTidy: opening the formatting settings…");
                url = await Task.Run(() => Pages.Open(exe, root));
            }
            catch (FormatterException ex)
            {
                await VS.StatusBar.ClearAsync();
                await VS.MessageBox.ShowErrorAsync("TcTidy", "Could not open the formatting settings: " + ex.Message);
                return;
            }

            await Browser.OpenAsync(url);
            await VS.StatusBar.ClearAsync();
        }
    }

    [Command(PackageGuids.guidTcTidyCmdSetString, PackageIds.CheckForUpdatesCommandId)]
    internal sealed class CheckForUpdatesCommand : BaseCommand<CheckForUpdatesCommand>
    {
        protected override Task ExecuteAsync(OleMenuCmdEventArgs e) => UpdateNotifier.CheckNowAsync();
    }

    [Command(PackageGuids.guidTcTidyCmdSetString, PackageIds.AboutCommandId)]
    internal sealed class AboutCommand : BaseCommand<AboutCommand>
    {
        protected override async Task ExecuteAsync(OleMenuCmdEventArgs e)
        {
            // Reuse the Format commands' engine resolution so the About box doubles as a
            // "where is my tctidy.exe" diagnostic; a missing engine is stated, never an error.
            var exe = FormatSupport.LocateExe();
            var engine = exe == null ? null : TctidyVersion.Read(exe);
            // LICENSE and the third-party notices are installed next to the extension assembly.
            var installFolder = Path.GetDirectoryName(typeof(TcTidyPackage).Assembly.Location);

            await VS.MessageBox.ShowAsync(
                "About TcTidy", AboutText.Build(ExtensionVersion.Current, engine, exe, installFolder, ExtensionVersion.Commit),
                OLEMSGICON.OLEMSGICON_INFO, OLEMSGBUTTON.OLEMSGBUTTON_OK);
        }
    }

    /// <summary>Opens a URL in the user's default browser, reporting rather than throwing on failure.</summary>
    internal static class Browser
    {
        public static async Task OpenAsync(string url)
        {
            try
            {
                Process.Start(new ProcessStartInfo(url) { UseShellExecute = true });
            }
            catch (Exception ex)
            {
                await VS.MessageBox.ShowErrorAsync("TcTidy", $"Could not open {url}: {ex.Message}");
            }
        }
    }

    /// <summary>
    /// The installed extension's full SemVer, synchronized from the workspace version.
    /// </summary>
    internal static class ExtensionVersion
    {
        public static string Current => AboutText.DisplayVersion(Raw);

        /// <summary>The stamped informational version as is (<c>0.0.0-dev</c> for a development build).</summary>
        public static string? Raw =>
            typeof(TcTidyPackage).Assembly
                .GetCustomAttribute<AssemblyInformationalVersionAttribute>()?
                .InformationalVersion;

        /// <summary>
        /// The commit this build came from. It is read from the Core assembly, the one the .NET SDK
        /// stamps; the shell's own informational version is the product version alone.
        /// </summary>
        public static string? Commit =>
            AboutText.Commit(
                typeof(AboutText).Assembly
                    .GetCustomAttribute<AssemblyInformationalVersionAttribute>()?
                    .InformationalVersion);
    }
}
