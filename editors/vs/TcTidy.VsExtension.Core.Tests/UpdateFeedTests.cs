using System;
using System.Collections.Generic;
using System.Net;
using System.Net.Http;
using System.Security.Authentication;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    /// <summary>
    /// Exercises <see cref="UpdateFeed"/> against canned HTTP responses: the manifest, its fallback on
    /// the latest-release redirect, and failures, which must end in no manifest and say why.
    /// </summary>
    public sealed class UpdateFeedTests
    {
        private const string Manifest =
            "{\"schema\":1,\"latest\":\"0.6.0\",\"url\":\"https://github.com/ViKodex/TcTidy/releases/tag/v0.6.0\"}";

        private sealed class FakeHandler : HttpMessageHandler
        {
            private readonly Func<HttpRequestMessage, HttpResponseMessage> respond;

            public FakeHandler(Func<HttpRequestMessage, HttpResponseMessage> respond) => this.respond = respond;

            public List<string> Requests { get; } = new List<string>();

            protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellationToken)
            {
                Requests.Add(request.Method + " " + request.RequestUri);
                return Task.FromResult(respond(request));
            }
        }

        private static HttpResponseMessage Json(string body) =>
            new HttpResponseMessage(HttpStatusCode.OK) { Content = new StringContent(body, Encoding.UTF8, "application/json") };

        private static HttpResponseMessage Redirect(string location)
        {
            var response = new HttpResponseMessage(HttpStatusCode.Found);
            response.Headers.Location = new Uri(location, UriKind.RelativeOrAbsolute);
            return response;
        }

        private static async Task<UpdateManifest?> Fetch(HttpMessageHandler handler) =>
            (await FetchResult(handler)).Manifest;

        private static Task<UpdateFetch> FetchResult(HttpMessageHandler handler) =>
            UpdateFeed.FetchAsync(handler, TimeSpan.FromSeconds(5), CancellationToken.None);

        private static bool IsManifest(HttpRequestMessage request) =>
            request.RequestUri!.Host.EndsWith("github.io", StringComparison.Ordinal);

        [Fact]
        public async Task Reads_the_manifest()
        {
            var handler = new FakeHandler(_ => Json(Manifest));

            var manifest = await Fetch(handler);

            Assert.Equal("0.6.0", manifest!.Latest.ToString());
            Assert.Equal(new[] { "GET " + ProjectLinks.UpdateManifestUrl }, handler.Requests);
        }

        [Fact]
        public async Task Follows_https_redirects_to_the_manifest()
        {
            var handler = new FakeHandler(r =>
                r.RequestUri!.AbsolutePath == "/moved.json" ? Json(Manifest) : Redirect("/moved.json"));

            Assert.NotNull(await Fetch(handler));
        }

        [Fact]
        public async Task Falls_back_on_the_latest_release_redirect()
        {
            var handler = new FakeHandler(r =>
                IsManifest(r)
                    ? new HttpResponseMessage(HttpStatusCode.NotFound)
                    : Redirect("https://github.com/ViKodex/TcTidy/releases/tag/v0.7.0"));

            var manifest = await Fetch(handler);

            Assert.Equal("0.7.0", manifest!.Latest.ToString());
            Assert.Equal("https://github.com/ViKodex/TcTidy/releases/tag/v0.7.0", manifest.Url);
            Assert.Empty(manifest.Notices);
            Assert.Equal("HEAD " + ProjectLinks.LatestRelease, handler.Requests[1]);
        }

        [Fact]
        public async Task The_fallback_follows_a_moved_repository()
        {
            var handler = new FakeHandler(r =>
                IsManifest(r) ? Json("garbage")
                : r.RequestUri!.AbsolutePath.StartsWith("/ViKodex/", StringComparison.Ordinal)
                    ? Redirect("https://github.com/NewOwner/TcTidy/releases/latest")
                    : Redirect("https://github.com/NewOwner/TcTidy/releases/tag/v0.8.0"));

            Assert.Equal("0.8.0", (await Fetch(handler))!.Latest.ToString());
        }

        [Fact]
        public async Task A_redirect_loop_or_downgrade_ends_in_null()
        {
            Assert.Null(await Fetch(new FakeHandler(r => Redirect(r.RequestUri!.AbsoluteUri))));
            Assert.Null(await Fetch(new FakeHandler(_ => Redirect("http://github.com/ViKodex/TcTidy/releases/tag/v0.7.0"))));
        }

        [Fact]
        public async Task A_network_failure_ends_in_null()
        {
            Assert.Null(await Fetch(new FakeHandler(_ => throw new HttpRequestException("offline"))));
        }

        [Fact]
        public async Task A_success_reports_no_failure()
        {
            Assert.Null((await FetchResult(new FakeHandler(_ => Json(Manifest)))).Failure);
        }

        [Fact]
        public async Task A_failure_names_each_source_and_its_status()
        {
            var result = await FetchResult(new FakeHandler(_ => new HttpResponseMessage(HttpStatusCode.NotFound)));

            Assert.Equal(
                ProjectLinks.UpdateManifestUrl + ": HTTP 404\n" + ProjectLinks.LatestRelease + ": HTTP 404",
                result.Failure);
        }

        [Fact]
        public async Task A_failure_reports_the_innermost_error()
        {
            var tls = new HttpRequestException(
                "An error occurred while sending the request.",
                new WebException("Could not create SSL/TLS secure channel."));

            var result = await FetchResult(new FakeHandler(_ => throw tls));

            Assert.Equal(
                ProjectLinks.UpdateManifestUrl + ": Could not create SSL/TLS secure channel.\n"
                + ProjectLinks.LatestRelease + ": Could not create SSL/TLS secure channel.",
                result.Failure);
        }

        [Fact]
        public async Task A_failure_tells_a_timeout_from_a_cancellation()
        {
            var result = await FetchResult(new FakeHandler(_ => throw new TaskCanceledException()));

            Assert.Contains(ProjectLinks.UpdateManifestUrl + ": timed out", result.Failure);
        }

        [Fact]
        public async Task A_cancellation_is_not_reported_as_a_failure()
        {
            using var cancel = new CancellationTokenSource();
            cancel.Cancel();

            await Assert.ThrowsAnyAsync<OperationCanceledException>(() => UpdateFeed.FetchAsync(
                new FakeHandler(_ => throw new TaskCanceledException()), TimeSpan.FromSeconds(5), cancel.Token));
        }

        [Fact]
        public async Task A_failure_explains_an_unreadable_manifest_and_a_missing_release()
        {
            var result = await FetchResult(new FakeHandler(r =>
                IsManifest(r) ? Json("garbage") : new HttpResponseMessage(HttpStatusCode.OK)));

            Assert.Equal(
                ProjectLinks.UpdateManifestUrl + ": not a valid update manifest\n"
                + ProjectLinks.LatestRelease + ": no release found",
                result.Failure);
        }

        [Fact]
        public async Task A_failure_explains_a_redirect_loop_and_a_tag_that_is_no_version()
        {
            var loop = await FetchResult(new FakeHandler(r => Redirect(r.RequestUri!.AbsoluteUri)));
            Assert.Contains(ProjectLinks.UpdateManifestUrl + ": too many redirects", loop.Failure);
            Assert.Contains(ProjectLinks.LatestRelease + ": too many redirects", loop.Failure);

            var badTag = await FetchResult(new FakeHandler(r =>
                IsManifest(r)
                    ? new HttpResponseMessage(HttpStatusCode.NotFound)
                    : Redirect("https://github.com/ViKodex/TcTidy/releases/tag/nightly")));
            Assert.Contains(ProjectLinks.LatestRelease + ": the latest release is not tagged with a version", badTag.Failure);
        }

        [Fact]
        public async Task A_failure_reason_stays_on_one_line()
        {
            var result = await FetchResult(new FakeHandler(_ =>
                throw new HttpRequestException("first line\r\nsecond line")));

            Assert.Equal(
                ProjectLinks.UpdateManifestUrl + ": first line second line\n"
                + ProjectLinks.LatestRelease + ": first line second line",
                result.Failure);
        }

        [Fact]
        public void The_network_handler_asks_for_tls_12_and_follows_no_redirect()
        {
            using var handler = UpdateFeed.CreateHandler();

            Assert.Equal(SslProtocols.Tls12, handler.SslProtocols);
            Assert.False(handler.AllowAutoRedirect);
        }

        [Fact]
        public void The_failure_message_says_why_and_where_to_look()
        {
            var text = UpdateText.CheckFailed("https://example.test: HTTP 404");

            Assert.Contains("https://example.test: HTTP 404", text);
            Assert.Contains(ProjectLinks.LatestRelease, text);
        }

        [Theory]
        [InlineData("https://github.com/o/r/releases/tag/v1.2.3", "1.2.3")]
        [InlineData("https://github.com/o/r/releases/tag/1.2.3/", "1.2.3")]
        [InlineData("https://github.com/o/r/releases", null)]
        [InlineData("https://github.com/o/r/releases/latest", null)]
        public void TagOf_reads_the_release_tag(string url, string? expected)
        {
            Assert.Equal(expected, UpdateFeed.TagOf(new Uri(url)));
        }
    }
}
