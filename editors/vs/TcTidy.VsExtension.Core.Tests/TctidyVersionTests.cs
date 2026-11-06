using System;
using System.IO;
using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    /// <summary>
    /// Exercises <see cref="TctidyVersion"/> against the real tctidy binary. Point the
    /// <c>TCTIDY_BIN</c> environment variable at a built binary to run these.
    /// </summary>
    public sealed class TctidyVersionTests
    {
        private static string TctidyPath()
        {
            var p = Environment.GetEnvironmentVariable("TCTIDY_BIN");
            if (string.IsNullOrEmpty(p) || !File.Exists(p))
            {
                throw new InvalidOperationException(
                    "Set TCTIDY_BIN to a built tctidy binary to run these tests.");
            }
            return p!;
        }

        [Fact]
        public void Read_returns_the_engine_version()
        {
            var version = TctidyVersion.Read(TctidyPath());

            Assert.NotNull(version);
            // "X.Y.Z" with an optional SemVer suffix: the "tctidy " prefix is stripped.
            Assert.Matches(@"^\d+\.\d+\.\d+([-+][0-9A-Za-z.+-]+)?$", version);
        }

        [Fact]
        public void Read_returns_null_for_a_missing_exe()
        {
            var missing = Path.Combine(Path.GetTempPath(), $"tctidy-missing-{Guid.NewGuid():N}.exe");

            Assert.Null(TctidyVersion.Read(missing));
        }

        [Theory]
        [InlineData("tctidy 0.3.0", "0.3.0")]
        [InlineData("tctidy 0.3.0\r\n", "0.3.0")]
        [InlineData("tctidy 0.4.0-rc.1", "0.4.0-rc.1")]
        [InlineData("tctidy 0.4.0+abc", "0.4.0+abc")]
        [InlineData("tctidy 0.4.0-rc.1+abc.5", "0.4.0-rc.1+abc.5")]
        public void Parse_keeps_the_full_version_including_a_prerelease_or_build_suffix(
            string output, string expected)
        {
            Assert.Equal(expected, TctidyVersion.Parse(output));
        }

        [Theory]
        [InlineData("")]
        [InlineData("tctidy")]
        [InlineData("tctidy rc.1")]
        [InlineData("tctidy -rc.1")]
        [InlineData("tctidy 0.4.0-")]
        [InlineData("error: unexpected argument")]
        public void Parse_returns_null_for_output_that_is_not_a_version(string output)
        {
            Assert.Null(TctidyVersion.Parse(output));
        }
    }
}
