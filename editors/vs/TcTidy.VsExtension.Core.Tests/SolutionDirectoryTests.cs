using System;
using System.IO;
using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    /// <summary>Exercises <see cref="SolutionDirectory"/>: where a solution's settings belong.</summary>
    public sealed class SolutionDirectoryTests
    {
        [Theory]
        [InlineData(null)]
        [InlineData("")]
        [InlineData("   ")]
        public void No_saved_solution_has_no_directory(string? solutionFile)
        {
            Assert.Null(SolutionDirectory.Of(solutionFile));
        }

        [Fact]
        public void A_solution_lives_in_the_directory_of_its_sln()
        {
            var dir = Path.Combine(Path.GetTempPath(), $"tctidy-sln-{Guid.NewGuid():N}");
            Directory.CreateDirectory(dir);
            try
            {
                Assert.Equal(dir, SolutionDirectory.Of(Path.Combine(dir, "Project.sln")));
            }
            finally { Directory.Delete(dir); }
        }

        [Fact]
        public void A_solution_whose_directory_is_gone_has_none()
        {
            var gone = Path.Combine(Path.GetTempPath(), $"tctidy-gone-{Guid.NewGuid():N}", "Project.sln");
            Assert.Null(SolutionDirectory.Of(gone));
        }

        [Fact]
        public void Same_ignores_case_and_a_trailing_separator()
        {
            var dir = Path.Combine(Path.GetTempPath(), "Solution");
            Assert.True(SolutionDirectory.Same(dir, dir.ToUpperInvariant() + Path.DirectorySeparatorChar));
            Assert.False(SolutionDirectory.Same(dir, dir + "2"));
        }
    }
}
