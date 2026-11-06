using System;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// The project's public URLs, one place to change if the repository ever moves. VS-independent
    /// so the URL building (bug-report form, prefilled title and version) is unit-testable.
    /// </summary>
    public static class ProjectLinks
    {
        public const string Repository = "https://github.com/ViKodex/TcTidy";

        /// <summary>The extension's user documentation, the repository README.</summary>
        public const string Documentation = Repository + "#readme";

        /// <summary>
        /// The update manifest (<see cref="UpdateManifest"/>), published to GitHub Pages by
        /// <c>.github/workflows/update-manifest.yml</c>. Every installed extension reads this exact
        /// URL, so it must keep serving the file even if the repository moves.
        /// </summary>
        public const string UpdateManifestUrl = "https://vikodex.github.io/TcTidy/update.json";

        /// <summary>Redirects to the latest stable release's page; the manifest's fallback.</summary>
        public const string LatestRelease = Repository + "/releases/latest";

        /// <summary>
        /// A new GitHub issue on the bug-report form (<c>.github/ISSUE_TEMPLATE/bug_report.yml</c>).
        /// When the extension version is known it is prefilled in the title (<c>[VSIX X.Y.Z] </c>) and
        /// in the form's <c>version</c> field, the one piece of context a reporter always forgets; a
        /// null version just opens the empty form.
        /// </summary>
        public static string NewBugReport(string? extensionVersion)
        {
            var url = Repository + "/issues/new?template=bug_report.yml";
            if (!string.IsNullOrEmpty(extensionVersion))
            {
                url += "&title=" + Uri.EscapeDataString($"[VSIX {extensionVersion}] ");
                url += "&version=" + Uri.EscapeDataString(extensionVersion!);
            }
            return url;
        }
    }
}
