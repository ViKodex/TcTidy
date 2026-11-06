using System.Collections.Generic;
using System.IO;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// Locates the <c>tctidy</c> executable. VS-independent so it can be unit-tested without
    /// an IDE: the installer drops <c>tctidy.exe</c> next to the extension assembly, and a
    /// developer build can fall back to one on <c>PATH</c>.
    /// </summary>
    public static class TctidyLocator
    {
        /// <summary>The executable file name shipped beside the extension.</summary>
        public const string ExeName = "tctidy.exe";

        /// <summary>
        /// Return the first existing <c>tctidy.exe</c>, next to the assembly, then in each
        /// <paramref name="pathDirs"/> entry, or <c>null</c> if none is found.
        /// </summary>
        public static string? Resolve(string assemblyDir, IEnumerable<string> pathDirs)
        {
            if (!string.IsNullOrEmpty(assemblyDir))
            {
                var local = Path.Combine(assemblyDir, ExeName);
                if (File.Exists(local)) return local;
            }

            if (pathDirs != null)
            {
                foreach (var dir in pathDirs)
                {
                    // PATH entries may be wrapped in literal quotes (the OS strips them); do the same.
                    var d = dir?.Trim().Trim('"');
                    if (string.IsNullOrEmpty(d)) continue;
                    var candidate = Path.Combine(d, ExeName);
                    if (File.Exists(candidate)) return candidate;
                }
            }

            return null;
        }
    }
}
