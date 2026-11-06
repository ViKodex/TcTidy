using Community.VisualStudio.Toolkit;

namespace TcTidy.VsExtension
{
    /// <summary>
    /// The extension's persisted options, backed by the shell's settings store via the
    /// Toolkit's <see cref="BaseOptionModel{T}"/>, so they survive restarts with no
    /// extra registry or file plumbing.
    /// </summary>
    internal sealed class GeneralOptions : BaseOptionModel<GeneralOptions>
    {
        /// <summary>
        /// Write an end-to-end timing report after each Format run (see <see cref="PerfMode"/>).
        /// Toggled from the TcTidy menu; off by default so normal use is untouched.
        /// </summary>
        public bool PerfReportEnabled { get; set; }

        /// <summary>
        /// Check for a new release when the package loads (see <see cref="UpdateNotifier"/>). On by
        /// default; toggled from the TcTidy menu.
        /// </summary>
        public bool UpdateCheckEnabled { get; set; } = true;

        /// <summary>When the last update check succeeded, round-trip UTC text (<c>UpdateSettings.Stamp</c>).</summary>
        public string LastUpdateCheck { get; set; } = "";

        /// <summary>The release the user chose to skip; a later one is offered again.</summary>
        public string SkippedUpdateVersion { get; set; } = "";

        /// <summary>The ids of the dismissed update notices, one per line.</summary>
        public string DismissedUpdateNotices { get; set; } = "";
    }
}
