using System;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// A <c>MAJOR.MINOR.PATCH[-prerelease][+build]</c> version, ordered for update checks: the
    /// numeric core first, then a prerelease ("0.5.0-rc.1") before its release. Two prereleases of
    /// one core compare by ordinal text, enough here since the update manifest only names stable
    /// releases. Build metadata is ignored.
    /// </summary>
    public sealed class SemanticVersion : IComparable<SemanticVersion>
    {
        private SemanticVersion(Version core, string prerelease)
        {
            Core = core;
            Prerelease = prerelease;
        }

        public Version Core { get; }

        /// <summary>The prerelease label without its leading <c>-</c>, empty for a release.</summary>
        public string Prerelease { get; }

        public static SemanticVersion? TryParse(string? text)
        {
            if (string.IsNullOrWhiteSpace(text)) return null;

            var value = text!.Trim();
            var build = value.IndexOf('+');
            if (build >= 0) value = value.Substring(0, build);
            var dash = value.IndexOf('-');
            var core = dash < 0 ? value : value.Substring(0, dash);
            var prerelease = dash < 0 ? "" : value.Substring(dash + 1);
            if (dash >= 0 && prerelease.Length == 0) return null;

            // Exactly three numeric parts: Version also accepts "1.2" and "1.2.3.4".
            if (core.Split('.').Length != 3 || !Version.TryParse(core, out var parsed)) return null;
            return new SemanticVersion(parsed, prerelease);
        }

        public int CompareTo(SemanticVersion? other)
        {
            if (other is null) return 1;
            var core = Core.CompareTo(other.Core);
            if (core != 0) return core;
            if (Prerelease.Length == 0) return other.Prerelease.Length == 0 ? 0 : 1;
            if (other.Prerelease.Length == 0) return -1;
            return Math.Sign(string.CompareOrdinal(Prerelease, other.Prerelease));
        }

        public override string ToString() =>
            Prerelease.Length == 0 ? Core.ToString() : Core + "-" + Prerelease;
    }
}
