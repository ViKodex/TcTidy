using System;
using System.IO;
using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    /// <summary>Exercises <see cref="TctidyLocator"/>. Pure file-system logic, no VS needed.</summary>
    public sealed class TctidyLocatorTests
    {
        [Fact]
        public void Resolve_finds_the_exe_next_to_the_assembly()
        {
            using var dir = new TempDir();
            var exe = dir.Touch(TctidyLocator.ExeName);

            var found = TctidyLocator.Resolve(dir.Path, Array.Empty<string>());

            Assert.Equal(exe, found);
        }

        [Fact]
        public void Resolve_falls_back_to_a_PATH_directory()
        {
            using var assemblyDir = new TempDir(); // no exe here
            using var pathDir = new TempDir();
            var exe = pathDir.Touch(TctidyLocator.ExeName);

            var found = TctidyLocator.Resolve(assemblyDir.Path, new[] { "", pathDir.Path });

            Assert.Equal(exe, found);
        }

        [Fact]
        public void Resolve_strips_surrounding_quotes_from_PATH_entries()
        {
            using var assemblyDir = new TempDir();
            using var pathDir = new TempDir();
            var exe = pathDir.Touch(TctidyLocator.ExeName);

            var found = TctidyLocator.Resolve(assemblyDir.Path, new[] { "\"" + pathDir.Path + "\"" });

            Assert.Equal(exe, found);
        }

        [Fact]
        public void Resolve_returns_null_when_the_exe_is_nowhere()
        {
            using var assemblyDir = new TempDir();
            using var pathDir = new TempDir();

            Assert.Null(TctidyLocator.Resolve(assemblyDir.Path, new[] { pathDir.Path }));
        }

        private sealed class TempDir : IDisposable
        {
            public string Path { get; }

            public TempDir()
            {
                Path = System.IO.Path.Combine(
                    System.IO.Path.GetTempPath(), $"tctidy-locator-{Guid.NewGuid():N}");
                Directory.CreateDirectory(Path);
            }

            public string Touch(string name)
            {
                var file = System.IO.Path.Combine(Path, name);
                File.WriteAllText(file, string.Empty);
                return file;
            }

            public void Dispose()
            {
                try { Directory.Delete(Path, recursive: true); } catch { /* best effort */ }
            }
        }
    }
}
