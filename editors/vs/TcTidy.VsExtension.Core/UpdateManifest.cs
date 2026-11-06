using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Runtime.Serialization;
using System.Runtime.Serialization.Json;
using System.Text;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// The update manifest (<see cref="ProjectLinks.UpdateManifestUrl"/>): the latest stable release and
    /// the notices to show to installed versions. Every installed extension reads this format, so it
    /// only ever gains fields: unknown fields are ignored, and a breaking change would need a new file
    /// name. <c>schema</c> must be 1.
    /// <code>
    /// { "schema": 1, "latest": "X.Y.Z", "url": "https://...",
    ///   "notices": [ { "id": "...", "text": "...", "url": "https://...", "severity": "warning",
    ///                  "from": "X.Y.Z", "before": "X.Y.Z" } ] }
    /// </code>
    /// </summary>
    [DataContract]
    public sealed class UpdateManifest
    {
        [DataMember(Name = "schema")] private int schema;
        [DataMember(Name = "latest")] private string? latest;
        [DataMember(Name = "url")] private string? url;
        [DataMember(Name = "notices")] private List<UpdateNotice>? notices;

        private UpdateManifest(string latest, string url, List<UpdateNotice> notices)
        {
            schema = 1;
            this.latest = latest;
            this.url = url;
            this.notices = notices;
        }

        /// <summary>The latest stable release's version.</summary>
        public SemanticVersion Latest => SemanticVersion.TryParse(latest)!;

        /// <summary>The latest release's page, where the installer is downloaded.</summary>
        public string Url => url!;

        public IReadOnlyList<UpdateNotice> Notices => notices!;

        /// <summary>
        /// Reads a manifest, or null when it is not one this extension understands: invalid JSON, a
        /// schema other than 1, a missing or non-SemVer <c>latest</c>, or a <c>url</c> that is not
        /// https. Notices without an id or text are dropped, and a notice link that is not https is
        /// removed, since both URLs are opened in the browser.
        /// </summary>
        public static UpdateManifest? Parse(string json)
        {
            UpdateManifest? manifest;
            try
            {
                var serializer = new DataContractJsonSerializer(typeof(UpdateManifest));
                using var stream = new MemoryStream(Encoding.UTF8.GetBytes(json));
                manifest = serializer.ReadObject(stream) as UpdateManifest;
            }
            catch (Exception ex) when (ex is SerializationException || ex is InvalidCastException || ex is ArgumentException)
            {
                return null;
            }

            if (manifest == null
                || manifest.schema != 1
                || SemanticVersion.TryParse(manifest.latest) == null
                || !IsHttps(manifest.url))
            {
                return null;
            }

            manifest.notices = (manifest.notices ?? new List<UpdateNotice>())
                .Where(n => n != null && !string.IsNullOrWhiteSpace(n.Id) && !string.IsNullOrWhiteSpace(n.Text))
                .Select(n => n.WithUrl(IsHttps(n.Url) ? n.Url : null))
                .ToList();
            return manifest;
        }

        /// <summary>A manifest that only knows the latest release, for when the manifest is unreachable.</summary>
        public static UpdateManifest? FromRelease(string version, string releaseUrl) =>
            SemanticVersion.TryParse(version) != null && IsHttps(releaseUrl)
                ? new UpdateManifest(version, releaseUrl, new List<UpdateNotice>())
                : null;

        internal static bool IsHttps(string? value) =>
            Uri.TryCreate(value, UriKind.Absolute, out var uri) && uri.Scheme == Uri.UriSchemeHttps;
    }

    /// <summary>
    /// A message for installed versions in [<c>from</c>, <c>before</c>), either bound optional: a
    /// serious bug fixed in a later release, or a change the extension cannot announce by itself.
    /// The user dismisses it by <see cref="Id"/>, so a reworded notice keeps its id and a new one
    /// gets a new id.
    /// </summary>
    [DataContract]
    public sealed class UpdateNotice
    {
        // Assigned by the serializer only.
#pragma warning disable CS0649
        [DataMember(Name = "id")] private string? id;
        [DataMember(Name = "text")] private string? text;
        [DataMember(Name = "url")] private string? url;
        [DataMember(Name = "severity")] private string? severity;
        [DataMember(Name = "from")] private string? from;
        [DataMember(Name = "before")] private string? before;
#pragma warning restore CS0649

        public string Id => id!;

        public string Text => text!;

        /// <summary>An https link for more details, or null.</summary>
        public string? Url => url;

        /// <summary>Shown with a warning icon instead of an information icon.</summary>
        public bool IsWarning => string.Equals(severity, "warning", StringComparison.OrdinalIgnoreCase);

        /// <summary>
        /// Whether the notice is meant for <paramref name="installed"/>. A bound that is not a valid
        /// version makes the notice apply to nobody, so a typo never broadcasts it.
        /// </summary>
        public bool AppliesTo(SemanticVersion installed)
        {
            if (from != null)
            {
                var lower = SemanticVersion.TryParse(from);
                if (lower == null || installed.CompareTo(lower) < 0) return false;
            }
            if (before != null)
            {
                var upper = SemanticVersion.TryParse(before);
                if (upper == null || installed.CompareTo(upper) >= 0) return false;
            }
            return true;
        }

        internal UpdateNotice WithUrl(string? link)
        {
            url = link;
            return this;
        }
    }
}
