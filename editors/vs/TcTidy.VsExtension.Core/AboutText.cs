using System;
using System.Text;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// Builds the "About TcTidy" message box text. Pure string work, VS-independent and
    /// unit-tested; the shell only supplies the versions and shows the box.
    /// </summary>
    public static class AboutText
    {
        /// <summary>
        /// Maps the stamped informational version to user-facing text. Release metadata preserves
        /// its full SemVer, including a prerelease suffix; the development placeholder is rendered
        /// as a friendly label.
        /// </summary>
        public static string DisplayVersion(string? rawVersion)
        {
            if (string.IsNullOrEmpty(rawVersion))
            {
                return "unknown";
            }
            return rawVersion == "0.0.0-dev" ? "development" : rawVersion!;
        }

        /// <summary>
        /// The short commit from an informational version stamped by the .NET SDK
        /// (<c>1.0.0+a16f247...</c>), or null when it carries none.
        /// </summary>
        public static string? Commit(string? informationalVersion)
        {
            var plus = informationalVersion?.IndexOf('+') ?? -1;
            if (plus < 0) return null;
            var commit = informationalVersion!.Substring(plus + 1);
            return commit.Length == 0 ? null : commit.Substring(0, Math.Min(7, commit.Length));
        }

        /// <summary>The line that says TcTidy is not a Beckhoff product.</summary>
        public const string Independence =
            "TcTidy is an independent project, not affiliated with or endorsed by Beckhoff Automation.";

        /// <summary>
        /// The About box body: product, extension version and the commit it was built from (the one
        /// thing that tells two builds of a version apart), the tctidy engine's version and resolved
        /// path (doubling as a "where is my tctidy.exe" diagnostic), the folder holding the license and
        /// third-party notices (the extension's install folder), the <see cref="Independence"/> line,
        /// and the repository URL. A missing engine is stated outright, the same situation the Format
        /// commands error on.
        /// </summary>
        public static string Build(
            string extensionVersion, string? engineVersion, string? enginePath, string? noticesDirectory,
            string? commit = null)
        {
            var text = new StringBuilder();
            text.AppendLine("TcTidy for TwinCAT");
            text.AppendLine(
                commit == null ? $"Extension {extensionVersion}" : $"Extension {extensionVersion} (build {commit})");
            text.AppendLine(
                engineVersion == null || enginePath == null
                    ? "tctidy engine: not found"
                    : $"tctidy engine {engineVersion} ({enginePath})");
            text.AppendLine();
            if (!string.IsNullOrEmpty(noticesDirectory))
            {
                text.AppendLine($"License and third-party notices: {noticesDirectory}");
            }
            text.AppendLine(Independence);
            text.Append(ProjectLinks.Repository);
            return text.ToString();
        }
    }
}
