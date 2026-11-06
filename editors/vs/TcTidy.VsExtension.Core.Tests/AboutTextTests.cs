using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    /// <summary>Exercises <see cref="AboutText"/>. Pure string building, no VS needed.</summary>
    public sealed class AboutTextTests
    {
        [Fact]
        public void DisplayVersion_preserves_a_release_prerelease()
        {
            Assert.Equal("1.2.3-rc.4", AboutText.DisplayVersion("1.2.3-rc.4"));
        }

        [Fact]
        public void DisplayVersion_labels_the_development_placeholder()
        {
            Assert.Equal("development", AboutText.DisplayVersion("0.0.0-dev"));
        }

        [Fact]
        public void DisplayVersion_returns_unknown_for_missing_input()
        {
            Assert.Equal("unknown", AboutText.DisplayVersion(null));
            Assert.Equal("unknown", AboutText.DisplayVersion(""));
        }

        [Fact]
        public void Build_lists_product_extension_version_and_engine()
        {
            var text = AboutText.Build("1.2.3-rc.4", "1.2.3", @"C:\ext\tctidy.exe", @"C:\ext");

            Assert.Contains("TcTidy for TwinCAT", text);
            Assert.Contains("Extension 1.2.3-rc.4", text);
            Assert.Contains(@"tctidy engine 1.2.3 (C:\ext\tctidy.exe)", text);
            Assert.Contains(ProjectLinks.Repository, text);
        }

        [Theory]
        [InlineData("1.0.0+a16f247a5b876fdd3eceeeb68de2e018b52386d8", "a16f247")]
        [InlineData("1.0.0+abc", "abc")]
        [InlineData("1.0.0-rc.1+a16f247a5b", "a16f247")]
        [InlineData("1.0.0+", null)]
        [InlineData("1.0.0", null)]
        [InlineData("", null)]
        [InlineData(null, null)]
        public void Commit_reads_the_short_commit_of_an_informational_version(string? version, string? expected)
        {
            Assert.Equal(expected, AboutText.Commit(version));
        }

        [Fact]
        public void Build_shows_the_commit_next_to_the_extension_version()
        {
            var text = AboutText.Build("1.2.3", "1.2.3", @"C:\ext\tctidy.exe", @"C:\ext", "a16f247");

            Assert.Contains("Extension 1.2.3 (build a16f247)", text);
        }

        [Fact]
        public void Build_omits_an_unknown_commit()
        {
            var text = AboutText.Build("1.2.3", "1.2.3", @"C:\ext\tctidy.exe", @"C:\ext");

            Assert.Contains("Extension 1.2.3", text);
            Assert.DoesNotContain("build", text);
        }

        [Fact]
        public void Build_says_tctidy_is_not_a_beckhoff_product()
        {
            var text = AboutText.Build("1.2.3", "1.2.3", @"C:\ext\tctidy.exe", null);

            Assert.Contains("not affiliated with or endorsed by Beckhoff Automation", text);
        }

        [Fact]
        public void Build_shows_a_prerelease_engine_version_in_full()
        {
            var text = AboutText.Build("0.4.0-rc.1", "0.4.0-rc.1", @"C:\ext\tctidy.exe", @"C:\ext");

            Assert.Contains(@"tctidy engine 0.4.0-rc.1 (C:\ext\tctidy.exe)", text);
        }

        [Fact]
        public void Build_says_where_the_license_and_third_party_notices_are_installed()
        {
            var text = AboutText.Build("1.2.3", "1.2.3", @"C:\ext\tctidy.exe", @"C:\ext");

            Assert.Contains(@"License and third-party notices: C:\ext", text);
        }

        [Fact]
        public void Build_omits_the_notices_line_when_the_folder_is_unknown()
        {
            var text = AboutText.Build("1.2.3", "1.2.3", @"C:\ext\tctidy.exe", null);

            Assert.DoesNotContain("third-party notices", text);
            Assert.Contains(ProjectLinks.Repository, text);
        }

        [Fact]
        public void Build_reports_a_missing_engine_as_not_found()
        {
            var text = AboutText.Build("development", null, null, @"C:\ext");

            Assert.Contains("tctidy engine: not found", text);
            Assert.DoesNotContain("()", text);
        }
    }
}
