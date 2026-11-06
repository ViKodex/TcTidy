using System;
using System.Net;
using System.Net.Http;
using System.Security.Authentication;
using System.Threading;
using System.Threading.Tasks;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// Downloads the update information: the <see cref="ProjectLinks.UpdateManifestUrl"/> first, and when
    /// it is unreachable or unreadable, the latest release read from the redirect of
    /// <see cref="ProjectLinks.LatestRelease"/>, which keeps working if the manifest host goes away and
    /// follows the repository if it moves. Anonymous requests for public URLs, no identifier sent.
    /// Best effort: any failure is reported in the <see cref="UpdateFetch"/>, never as an exception, so
    /// an offline machine or a blocking proxy costs nothing but the timeout.
    /// </summary>
    public static class UpdateFeed
    {
        private const int MaxRedirects = 5;
        private const int MaxManifestBytes = 64 * 1024;
        private const string TooManyRedirects = "too many redirects";

        /// <summary>Fetches over the network, through <see cref="CreateHandler"/>.</summary>
        public static async Task<UpdateFetch> FetchAsync(TimeSpan timeout, CancellationToken cancellationToken)
        {
            using var handler = CreateHandler();
            return await FetchAsync(handler, timeout, cancellationToken).ConfigureAwait(false);
        }

        /// <summary>
        /// The system proxy with the user's Windows credentials, as corporate proxies expect, and manual
        /// redirects, which the latest-release fallback needs. TLS 1.2 is requested on this handler
        /// alone: the shell's native executable can leave .NET on its legacy default protocols (SSL 3,
        /// TLS 1.0), which GitHub refuses, and the process-wide settings belong to the shell.
        /// </summary>
        public static HttpClientHandler CreateHandler() =>
            new HttpClientHandler
            {
                AllowAutoRedirect = false,
                DefaultProxyCredentials = CredentialCache.DefaultCredentials,
                SslProtocols = SslProtocols.Tls12,
            };

        /// <param name="handler">Must not follow redirects.</param>
        public static async Task<UpdateFetch> FetchAsync(
            HttpMessageHandler handler, TimeSpan timeout, CancellationToken cancellationToken)
        {
            using var client = new HttpClient(handler, disposeHandler: false)
            {
                Timeout = timeout,
                MaxResponseContentBufferSize = MaxManifestBytes,
            };
            client.DefaultRequestHeaders.UserAgent.ParseAdd("TcTidy-UpdateCheck");

            var manifest = await TryManifestAsync(client, cancellationToken).ConfigureAwait(false);
            if (manifest.Manifest != null) return manifest;

            var release = await TryLatestReleaseAsync(client, cancellationToken).ConfigureAwait(false);
            return release.Manifest != null
                ? release
                : UpdateFetch.Failed(manifest.Failure + "\n" + release.Failure);
        }

        private static async Task<UpdateFetch> TryManifestAsync(HttpClient client, CancellationToken cancellationToken)
        {
            const string source = ProjectLinks.UpdateManifestUrl;
            try
            {
                var uri = new Uri(source);
                for (var hop = 0; hop <= MaxRedirects; hop++)
                {
                    using var response = await client.GetAsync(uri, cancellationToken).ConfigureAwait(false);
                    var next = RedirectTarget(uri, response);
                    if (next != null)
                    {
                        uri = next;
                        continue;
                    }
                    if (!response.IsSuccessStatusCode) return Failed(source, Status(response));
                    var json = await response.Content.ReadAsStringAsync().ConfigureAwait(false);
                    var manifest = UpdateManifest.Parse(json);
                    return manifest != null
                        ? UpdateFetch.Read(manifest)
                        : Failed(source, "not a valid update manifest");
                }
                return Failed(source, TooManyRedirects);
            }
            catch (Exception ex) when (!(ex is OperationCanceledException) || !cancellationToken.IsCancellationRequested)
            {
                return Failed(source, Reason(ex));
            }
        }

        /// <summary>
        /// Follows the redirects of <see cref="ProjectLinks.LatestRelease"/> until one points at a
        /// release tag (<c>.../releases/tag/vX.Y.Z</c>), without downloading the page.
        /// </summary>
        private static async Task<UpdateFetch> TryLatestReleaseAsync(HttpClient client, CancellationToken cancellationToken)
        {
            const string source = ProjectLinks.LatestRelease;
            try
            {
                var uri = new Uri(source);
                for (var hop = 0; hop <= MaxRedirects; hop++)
                {
                    using var request = new HttpRequestMessage(HttpMethod.Head, uri);
                    using var response = await client.SendAsync(request, cancellationToken).ConfigureAwait(false);
                    var next = RedirectTarget(uri, response);
                    if (next == null)
                    {
                        return Failed(source, response.IsSuccessStatusCode ? "no release found" : Status(response));
                    }

                    var tag = TagOf(next);
                    if (tag != null)
                    {
                        var manifest = UpdateManifest.FromRelease(tag, next.AbsoluteUri);
                        return manifest != null
                            ? UpdateFetch.Read(manifest)
                            : Failed(source, "the latest release is not tagged with a version");
                    }
                    uri = next;
                }
                return Failed(source, TooManyRedirects);
            }
            catch (Exception ex) when (!(ex is OperationCanceledException) || !cancellationToken.IsCancellationRequested)
            {
                return Failed(source, Reason(ex));
            }
        }

        private static UpdateFetch Failed(string source, string reason) =>
            UpdateFetch.Failed(source + ": " + reason);

        private static string Status(HttpResponseMessage response) => "HTTP " + (int)response.StatusCode;

        /// <summary>
        /// A timeout, or the innermost error, which is the one that says what happened (a refused TLS
        /// handshake, an unknown host) where the outer ones only say that a request failed.
        /// </summary>
        private static string Reason(Exception error)
        {
            if (error is OperationCanceledException) return "timed out";
            while (error.InnerException != null) error = error.InnerException;
            return error.Message.Replace("\r", "").Replace('\n', ' ');
        }

        private static Uri? RedirectTarget(Uri current, HttpResponseMessage response)
        {
            var code = (int)response.StatusCode;
            if (code < 300 || code > 399 || response.Headers.Location == null) return null;
            var target = new Uri(current, response.Headers.Location);
            // Never downgrade: both final URLs end up opened in the browser.
            return target.Scheme == Uri.UriSchemeHttps ? target : null;
        }

        /// <summary>"/owner/repo/releases/tag/v1.2.3" → "1.2.3"; anything else is a null.</summary>
        public static string? TagOf(Uri uri)
        {
            var segments = uri.AbsolutePath.TrimEnd('/').Split('/');
            var n = segments.Length;
            if (n < 3 || segments[n - 3] != "releases" || segments[n - 2] != "tag") return null;
            var tag = Uri.UnescapeDataString(segments[n - 1]);
            return tag.StartsWith("v", StringComparison.Ordinal) ? tag.Substring(1) : tag;
        }
    }

    /// <summary>What <see cref="UpdateFeed"/> got: a manifest, or why there is none.</summary>
    public sealed class UpdateFetch
    {
        private UpdateFetch(UpdateManifest? manifest, string? failure)
        {
            Manifest = manifest;
            Failure = failure;
        }

        public UpdateManifest? Manifest { get; }

        /// <summary>One line per source tried, with what went wrong; null when a manifest was read.</summary>
        public string? Failure { get; }

        internal static UpdateFetch Read(UpdateManifest manifest) => new UpdateFetch(manifest, null);

        internal static UpdateFetch Failed(string failure) => new UpdateFetch(null, failure);
    }
}
