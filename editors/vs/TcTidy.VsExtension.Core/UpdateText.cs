namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// The update check's user-visible wording, in Core so it stays pinned by tests like the other
    /// menu texts. The shell only shows these strings.
    /// </summary>
    public static class UpdateText
    {
        public static string Available(SemanticVersion newVersion, string installedVersion) =>
            $"TcTidy {newVersion} is available (installed: {installedVersion}).";

        public static string UpToDate(string installedVersion) =>
            $"TcTidy {installedVersion} is up to date.";

        public static string CheckFailed(string? failure) =>
            "Could not check for TcTidy updates:\n\n" + failure + "\n\n" +
            "Check the network or proxy, or see the latest release at " + ProjectLinks.LatestRelease;

        public const string DevelopmentBuild =
            "This is a development build of TcTidy: it is not compared with the releases.";

        public const string DisabledByPolicy =
            "Update checks for TcTidy are turned off by your administrator.";

        public static string ToggleStatus(bool enabled) =>
            enabled
                ? "TcTidy: automatic update check on"
                : "TcTidy: automatic update check off - use Tools > TcTidy > Check for Updates";
    }
}
