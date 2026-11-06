using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    /// <summary>Exercises <see cref="SemanticVersion"/> parsing and update-check ordering.</summary>
    public sealed class SemanticVersionTests
    {
        [Theory]
        [InlineData("1.2.3", "1.2.3")]
        [InlineData(" 1.2.3 ", "1.2.3")]
        [InlineData("1.2.3-rc.1", "1.2.3-rc.1")]
        [InlineData("1.2.3+build.5", "1.2.3")]
        [InlineData("1.2.3-rc.1+build", "1.2.3-rc.1")]
        public void Parses_valid_versions(string text, string expected)
        {
            Assert.Equal(expected, SemanticVersion.TryParse(text)!.ToString());
        }

        [Theory]
        [InlineData(null)]
        [InlineData("")]
        [InlineData("1.2")]
        [InlineData("1.2.3.4")]
        [InlineData("v1.2.3")]
        [InlineData("1.2.3-")]
        [InlineData("development")]
        public void Rejects_anything_else(string? text)
        {
            Assert.Null(SemanticVersion.TryParse(text));
        }

        [Theory]
        [InlineData("0.5.0", "0.4.9", 1)]
        [InlineData("0.10.0", "0.9.0", 1)]
        [InlineData("1.0.0", "1.0.0", 0)]
        [InlineData("1.0.0", "1.0.0-rc.1", 1)]
        [InlineData("1.0.0-rc.1", "1.0.0", -1)]
        [InlineData("1.0.0-rc.2", "1.0.0-rc.1", 1)]
        [InlineData("1.0.1-rc.1", "1.0.0", 1)]
        [InlineData("1.0.0+a", "1.0.0+b", 0)]
        public void Orders_by_core_then_prerelease(string left, string right, int expected)
        {
            var a = SemanticVersion.TryParse(left)!;
            var b = SemanticVersion.TryParse(right)!;

            Assert.Equal(expected, a.CompareTo(b));
            Assert.Equal(-expected, b.CompareTo(a));
        }
    }
}
