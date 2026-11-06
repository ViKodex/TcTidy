using System;
using System.Collections.Generic;
using System.Threading;
using Community.VisualStudio.Toolkit;
using Microsoft.VisualStudio.Imaging;
using Microsoft.VisualStudio.Imaging.Interop;
using Microsoft.VisualStudio.Shell;
using Microsoft.VisualStudio.Shell.Interop;
using Microsoft.Win32;
using TcTidy.VsExtension.Core;
using Task = System.Threading.Tasks.Task;

namespace TcTidy.VsExtension
{
    /// <summary>
    /// Tells the user about new releases and notices from the update manifest (<see cref="UpdateFeed"/>).
    /// The automatic check runs in the background when the package loads (at shell startup, or with
    /// the first solution), at most once per <see cref="UpdateSettings.CheckInterval"/>, and only
    /// shows something when there is something to say: an info bar in the main window, falling back
    /// to the TcTidy Output pane when the shell has no info bar host. It only informs: the user downloads and runs the installer.
    /// <para>
    /// Off when the user unticks <i>Check for Updates Automatically</i>, or machine-wide when the
    /// <c>DisableUpdateCheck</c> policy value is set (<see cref="DisabledByPolicy"/>), which also
    /// blocks the manual check: the policy exists for machines that must not reach the network.
    /// </para>
    /// </summary>
    internal static class UpdateNotifier
    {
        private const string PolicyKey = @"SOFTWARE\Policies\ViKodex\TcTidy";
        private const string PolicyValue = "DisableUpdateCheck";
        private static readonly TimeSpan Timeout = TimeSpan.FromSeconds(15);

        // Action contexts of the info bar links.
        private const string OpenRelease = "open-release";
        private const string SkipRelease = "skip-release";
        private const string OpenNotice = "open-notice";
        private const string DismissNotice = "dismiss-notice";

        // The bars on screen, by text, so a manual check during the load-time one (or a second
        // manual check) does not stack a copy of a bar that is still open. UI thread only.
        private static readonly Dictionary<string, InfoBar> Bars = new Dictionary<string, InfoBar>();

        /// <summary>
        /// <c>HKLM\SOFTWARE\Policies\ViKodex\TcTidy</c>, DWORD <c>DisableUpdateCheck</c> non-zero. The
        /// Policies key is shared by the 32- and 64-bit registry views, so both shell generations read
        /// the same value.
        /// </summary>
        public static bool DisabledByPolicy
        {
            get
            {
                try
                {
                    using var key = Registry.LocalMachine.OpenSubKey(PolicyKey);
                    return key?.GetValue(PolicyValue) is int value && value != 0;
                }
                catch
                {
                    return false;
                }
            }
        }

        /// <summary>
        /// The installed release version, or null for a development build (<c>0.0.0-dev</c>), which is
        /// never compared with the releases.
        /// </summary>
        private static SemanticVersion? InstalledVersion
        {
            get
            {
                var version = SemanticVersion.TryParse(ExtensionVersion.Raw);
                return version == null || version.Core == new Version(0, 0, 0) ? null : version;
            }
        }

        /// <summary>The background check at package load. Never throws.</summary>
        public static async Task CheckOnLoadAsync(CancellationToken cancellationToken)
        {
            try
            {
                var installed = InstalledVersion;
                if (installed == null || DisabledByPolicy) return;

                var options = await GeneralOptions.GetLiveInstanceAsync();
                if (!options.UpdateCheckEnabled
                    || !UpdateSettings.IsCheckDue(options.LastUpdateCheck, DateTime.UtcNow))
                {
                    return;
                }

                var manifest = (await Task.Run(() => FetchAsync(cancellationToken))).Manifest;
                if (manifest == null) return;

                options.LastUpdateCheck = UpdateSettings.Stamp(DateTime.UtcNow);
                await options.SaveAsync();

                var advice = UpdateAdvice.Evaluate(
                    manifest, installed, options.SkippedUpdateVersion,
                    UpdateSettings.ParseIds(options.DismissedUpdateNotices));
                await ShowAsync(advice, installed);
            }
            catch
            {
                // best effort: an update check never disturbs the shell
            }
        }

        /// <summary>
        /// Tools > TcTidy > Check for Updates: checks now, ignoring the schedule, the skipped release and
        /// dismissed notices, and always answers, with a message box when there is nothing to show.
        /// </summary>
        public static async Task CheckNowAsync()
        {
            if (DisabledByPolicy)
            {
                await VS.MessageBox.ShowAsync("TcTidy", UpdateText.DisabledByPolicy);
                return;
            }
            var installed = InstalledVersion;
            if (installed == null)
            {
                await VS.MessageBox.ShowAsync("TcTidy", UpdateText.DevelopmentBuild);
                return;
            }

            await VS.StatusBar.ShowMessageAsync("TcTidy: checking for updates…");
            var fetch = await Task.Run(() => FetchAsync(CancellationToken.None));
            await VS.StatusBar.ClearAsync();
            var manifest = fetch.Manifest;
            if (manifest == null)
            {
                await VS.MessageBox.ShowErrorAsync("TcTidy", UpdateText.CheckFailed(fetch.Failure));
                return;
            }

            var options = await GeneralOptions.GetLiveInstanceAsync();
            options.LastUpdateCheck = UpdateSettings.Stamp(DateTime.UtcNow);
            await options.SaveAsync();

            var advice = UpdateAdvice.Evaluate(manifest, installed);
            if (advice.NewVersion == null && advice.Notices.Count == 0)
            {
                await VS.MessageBox.ShowAsync("TcTidy", UpdateText.UpToDate(installed.ToString()));
                return;
            }
            await ShowAsync(advice, installed);
        }

        private static System.Threading.Tasks.Task<UpdateFetch> FetchAsync(CancellationToken cancellationToken) =>
            UpdateFeed.FetchAsync(Timeout, cancellationToken);

        private static async Task ShowAsync(UpdateAdvice advice, SemanticVersion installed)
        {
            await ThreadHelper.JoinableTaskFactory.SwitchToMainThreadAsync();

            if (advice.NewVersion != null)
            {
                var newVersion = advice.NewVersion.ToString();
                await ShowBarAsync(
                    UpdateText.Available(advice.NewVersion, installed.ToString()),
                    advice.ReleaseUrl,
                    KnownMonikers.StatusInformation,
                    new InfoBarHyperlink("Download", OpenRelease),
                    new InfoBarHyperlink("Skip this version", SkipRelease),
                    async context =>
                    {
                        if (context == OpenRelease)
                        {
                            await Browser.OpenAsync(advice.ReleaseUrl);
                        }
                        else
                        {
                            var options = await GeneralOptions.GetLiveInstanceAsync();
                            options.SkippedUpdateVersion = newVersion;
                            await options.SaveAsync();
                        }
                    });
            }

            foreach (var notice in advice.Notices)
            {
                await ShowBarAsync(
                    "TcTidy: " + notice.Text,
                    notice.Url,
                    notice.IsWarning ? KnownMonikers.StatusWarning : KnownMonikers.StatusInformation,
                    notice.Url == null ? null : new InfoBarHyperlink("Learn more", OpenNotice),
                    new InfoBarHyperlink("Don't show again", DismissNotice),
                    async context =>
                    {
                        if (context == OpenNotice)
                        {
                            await Browser.OpenAsync(notice.Url!);
                        }
                        else
                        {
                            var options = await GeneralOptions.GetLiveInstanceAsync();
                            options.DismissedUpdateNotices = UpdateSettings.AddId(options.DismissedUpdateNotices, notice.Id);
                            await options.SaveAsync();
                        }
                    });
            }
        }

        /// <summary>
        /// One info bar with an optional open link and a dismiss link, closed by either, unless the same
        /// bar is still open. Without an info bar host the message goes to the TcTidy Output pane with
        /// its link instead.
        /// </summary>
        private static async Task ShowBarAsync(
            string text, string? link, ImageMoniker icon,
            InfoBarHyperlink? open, InfoBarHyperlink dismiss, Func<string, Task> onAction)
        {
            if (Bars.TryGetValue(text, out var shown) && shown.IsVisible) return;

            var actions = open == null ? new[] { dismiss } : new[] { open, dismiss };
            var bar = await VS.InfoBar.CreateAsync(new InfoBarModel(text, actions, icon, isCloseButtonVisible: true));
            if (bar != null)
            {
                bar.ActionItemClicked += (_, e) =>
                {
                    ThreadHelper.ThrowIfNotOnUIThread();
                    var context = e.ActionItem.ActionContext as string ?? "";
                    e.InfoBarUIElement.Close();
                    onAction(context).FireAndForget();
                };
                if (await bar.TryShowInfoBarUIAsync())
                {
                    Bars[text] = bar;
                    return;
                }
            }

            await TcTidyOutput.WriteLineAsync(link == null ? text : text + " " + link);
            await VS.StatusBar.ShowMessageAsync(text);
        }
    }
}
