using System.Linq;
using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    /// <summary>
    /// Exercises <see cref="UpdateManifest"/>, <see cref="UpdateNotice"/> and <see cref="UpdateAdvice"/>:
    /// the format every installed extension reads, and what a check offers from it.
    /// </summary>
    public sealed class UpdateManifestTests
    {
        private const string ReleaseUrl = "https://github.com/ViKodex/TcTidy/releases/tag/v0.6.0";

        private static UpdateManifest Manifest(string notices = "[]") =>
            UpdateManifest.Parse(
                "{\"schema\":1,\"latest\":\"0.6.0\",\"url\":\"" + ReleaseUrl + "\",\"notices\":" + notices + "}")!;

        private static SemanticVersion V(string text) => SemanticVersion.TryParse(text)!;

        [Fact]
        public void Parses_the_manifest_and_ignores_unknown_fields()
        {
            var manifest = UpdateManifest.Parse(
                "{\"schema\":1,\"latest\":\"0.6.0\",\"url\":\"" + ReleaseUrl + "\",\"installer\":{\"sha256\":\"ab\"}}");

            Assert.NotNull(manifest);
            Assert.Equal("0.6.0", manifest!.Latest.ToString());
            Assert.Equal(ReleaseUrl, manifest.Url);
            Assert.Empty(manifest.Notices);
        }

        [Theory]
        [InlineData("not json")]
        [InlineData("[]")]
        [InlineData("{\"schema\":2,\"latest\":\"0.6.0\",\"url\":\"" + ReleaseUrl + "\"}")]
        [InlineData("{\"latest\":\"0.6.0\",\"url\":\"" + ReleaseUrl + "\"}")]
        [InlineData("{\"schema\":1,\"latest\":\"latest\",\"url\":\"" + ReleaseUrl + "\"}")]
        [InlineData("{\"schema\":1,\"latest\":\"0.6.0\",\"url\":\"http://example.com\"}")]
        [InlineData("{\"schema\":1,\"latest\":\"0.6.0\"}")]
        [InlineData("{\"schema\":\"one\",\"latest\":\"0.6.0\",\"url\":\"" + ReleaseUrl + "\"}")]
        public void Rejects_a_manifest_it_does_not_understand(string json)
        {
            Assert.Null(UpdateManifest.Parse(json));
        }

        [Fact]
        public void Drops_incomplete_notices_and_non_https_links()
        {
            var manifest = Manifest(
                "[{\"id\":\"a\",\"text\":\"A\",\"url\":\"http://example.com\"}," +
                "{\"id\":\"\",\"text\":\"B\"},{\"id\":\"c\"},null," +
                "{\"id\":\"d\",\"text\":\"D\",\"url\":\"https://example.com/d\",\"severity\":\"Warning\"}]");

            Assert.Equal(new[] { "a", "d" }, manifest.Notices.Select(n => n.Id));
            Assert.Null(manifest.Notices[0].Url);
            Assert.False(manifest.Notices[0].IsWarning);
            Assert.Equal("https://example.com/d", manifest.Notices[1].Url);
            Assert.True(manifest.Notices[1].IsWarning);
        }

        [Theory]
        [InlineData("0.4.9", true)]
        [InlineData("0.5.0", true)]
        [InlineData("0.5.2", false)]
        [InlineData("0.4.0", false)]
        [InlineData("0.5.2-rc.1", true)]
        public void A_notice_applies_from_its_lower_bound_until_its_upper_bound(string installed, bool applies)
        {
            var notice = Manifest("[{\"id\":\"n\",\"text\":\"T\",\"from\":\"0.4.1\",\"before\":\"0.5.2\"}]").Notices[0];

            Assert.Equal(applies, notice.AppliesTo(V(installed)));
        }

        [Fact]
        public void A_notice_with_an_invalid_bound_applies_to_nobody()
        {
            var notice = Manifest("[{\"id\":\"n\",\"text\":\"T\",\"before\":\"soon\"}]").Notices[0];

            Assert.False(notice.AppliesTo(V("0.1.0")));
        }

        [Fact]
        public void FromRelease_builds_a_manifest_without_notices()
        {
            var manifest = UpdateManifest.FromRelease("0.6.0", ReleaseUrl)!;

            Assert.Equal("0.6.0", manifest.Latest.ToString());
            Assert.Empty(manifest.Notices);
            Assert.Null(UpdateManifest.FromRelease("next", ReleaseUrl));
            Assert.Null(UpdateManifest.FromRelease("0.6.0", "http://example.com"));
        }

        [Theory]
        [InlineData("0.5.0", "0.6.0")]
        [InlineData("0.6.0-rc.1", "0.6.0")]
        [InlineData("0.6.0", null)]
        [InlineData("0.7.0", null)]
        public void Offers_only_a_newer_release(string installed, string? offered)
        {
            var advice = UpdateAdvice.Evaluate(Manifest(), V(installed));

            Assert.Equal(offered, advice.NewVersion?.ToString());
            Assert.Equal(ReleaseUrl, advice.ReleaseUrl);
        }

        [Fact]
        public void A_skipped_release_is_not_offered_again_but_a_later_one_is()
        {
            Assert.Null(UpdateAdvice.Evaluate(Manifest(), V("0.5.0"), skippedVersion: "0.6.0").NewVersion);
            Assert.Equal("0.6.0", UpdateAdvice.Evaluate(Manifest(), V("0.5.0"), skippedVersion: "0.5.5").NewVersion?.ToString());
        }

        [Fact]
        public void Lists_the_applicable_notices_that_were_not_dismissed()
        {
            var manifest = Manifest(
                "[{\"id\":\"old\",\"text\":\"T\",\"before\":\"0.5.0\"}," +
                "{\"id\":\"all\",\"text\":\"T\"},{\"id\":\"gone\",\"text\":\"T\"}]");

            var advice = UpdateAdvice.Evaluate(manifest, V("0.5.0"), dismissedNotices: new[] { "gone" });

            Assert.Equal(new[] { "all" }, advice.Notices.Select(n => n.Id));
        }
    }
}
