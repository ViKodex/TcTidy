using System;
using System.IO;

namespace TcTidy.VsExtension.Core
{
    /// <summary>Where a solution's <c>.tctidy.toml</c> belongs: next to its <c>.sln</c>.</summary>
    public static class SolutionDirectory
    {
        /// <summary>
        /// The directory of <paramref name="solutionFile"/>, or null when no saved solution is open
        /// (an empty or invalid path, or a directory that does not exist).
        /// </summary>
        public static string? Of(string? solutionFile)
        {
            if (string.IsNullOrWhiteSpace(solutionFile)) return null;
            try
            {
                var dir = Path.GetDirectoryName(Path.GetFullPath(solutionFile));
                return !string.IsNullOrEmpty(dir) && Directory.Exists(dir) ? dir : null;
            }
            catch (Exception e) when (e is ArgumentException || e is NotSupportedException
                                      || e is PathTooLongException || e is System.Security.SecurityException)
            {
                return null;
            }
        }

        /// <summary>Whether two directory paths name the same directory (Windows: case-insensitive).</summary>
        public static bool Same(string a, string b) =>
            string.Equals(Normalize(a), Normalize(b), StringComparison.OrdinalIgnoreCase);

        private static string Normalize(string path)
        {
            try { path = Path.GetFullPath(path); }
            catch (Exception e) when (e is ArgumentException || e is NotSupportedException || e is PathTooLongException)
            {
                // Compare as given.
            }
            return path.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
        }
    }
}
