using System.Collections.Generic;
using System.Linq;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// What an update check tells the user: a newer release, if any, and the notices meant for the
    /// installed version. Pure decision logic over a parsed <see cref="UpdateManifest"/>.
    /// </summary>
    public sealed class UpdateAdvice
    {
        private UpdateAdvice(SemanticVersion? newVersion, string releaseUrl, IReadOnlyList<UpdateNotice> notices)
        {
            NewVersion = newVersion;
            ReleaseUrl = releaseUrl;
            Notices = notices;
        }

        /// <summary>The newer release to offer, or null when the installed version is current.</summary>
        public SemanticVersion? NewVersion { get; }

        public string ReleaseUrl { get; }

        public IReadOnlyList<UpdateNotice> Notices { get; }

        /// <summary>
        /// Compares <paramref name="installed"/> with the manifest. <paramref name="skippedVersion"/>
        /// (the release the user chose to skip) is not offered again, but a later one is; dismissed
        /// notices are left out. A manual check passes neither, so it reports everything.
        /// </summary>
        public static UpdateAdvice Evaluate(
            UpdateManifest manifest,
            SemanticVersion installed,
            string? skippedVersion = null,
            IEnumerable<string>? dismissedNotices = null)
        {
            var latest = manifest.Latest;
            var skipped = SemanticVersion.TryParse(skippedVersion);
            var offer = latest.CompareTo(installed) > 0 && (skipped == null || latest.CompareTo(skipped) > 0)
                ? latest
                : null;

            var dismissed = new HashSet<string>(dismissedNotices ?? Enumerable.Empty<string>());
            var notices = manifest.Notices
                .Where(n => n.AppliesTo(installed) && !dismissed.Contains(n.Id))
                .ToList();
            return new UpdateAdvice(offer, manifest.Url, notices);
        }
    }
}
