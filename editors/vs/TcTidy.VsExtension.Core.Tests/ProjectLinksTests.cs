using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    /// <summary>Exercises <see cref="ProjectLinks"/>. Pure URL building, no VS needed.</summary>
    public sealed class ProjectLinksTests
    {
        [Fact]
        public void NewBugReport_targets_the_bug_form()
        {
            var url = ProjectLinks.NewBugReport("1.2.3");

            Assert.StartsWith(ProjectLinks.Repository + "/issues/new?", url);
            Assert.Contains("template=bug_report.yml", url);
        }

        [Fact]
        public void NewBugReport_prefills_the_extension_version_in_the_title()
        {
            var url = ProjectLinks.NewBugReport("1.2.3");

            Assert.Contains("title=%5BVSIX%201.2.3%5D%20", url);
        }

        [Fact]
        public void NewBugReport_prefills_the_version_field_of_the_form()
        {
            var url = ProjectLinks.NewBugReport("1.2.3-rc.1");

            Assert.Contains("&version=1.2.3-rc.1", url);
        }

        [Fact]
        public void NewBugReport_omits_the_title_and_version_when_version_is_null()
        {
            var url = ProjectLinks.NewBugReport(null);

            Assert.DoesNotContain("title=", url);
            Assert.DoesNotContain("version=", url);
            Assert.Contains("template=bug_report.yml", url);
        }

        [Fact]
        public void NewBugReport_url_encodes_the_version()
        {
            var url = ProjectLinks.NewBugReport("1.2.3+local build");

            Assert.Contains("title=%5BVSIX%201.2.3%2Blocal%20build%5D%20", url);
            Assert.Contains("&version=1.2.3%2Blocal%20build", url);
            Assert.DoesNotContain("+local", url);
        }
    }
}
