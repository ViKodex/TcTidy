using System;
using System.Collections.Generic;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// The stderr of one <c>tctidy --files-from -</c> run, split into per-file diagnostics and the
    /// lines that name no file. tctidy reports a file it left untouched as
    /// <c>tctidy: &lt;path&gt;: &lt;message&gt;</c>, echoing the path exactly as the list gave it, so
    /// a line belongs to a file only when the text between <c>tctidy: </c> and a <c>": "</c>
    /// separator is that file's complete path. A file whose path is a prefix, suffix or substring of
    /// another's therefore never takes the other's diagnostic. Windows paths compare as the file
    /// system and tctidy's own de-duplication do: ignoring case, with <c>/</c> and <c>\</c> alike.
    /// Pure, so it is unit-testable without tctidy.
    /// </summary>
    public sealed class BatchDiagnostics
    {
        private const string ToolPrefix = "tctidy: ";
        private const string FormattedPrefix = "formatted ";
        private const string Separator = ": ";

        private readonly Dictionary<string, string> _messages;
        private readonly bool _windowsPaths;

        private BatchDiagnostics(Dictionary<string, string> messages, IReadOnlyList<string> otherLines, bool windowsPaths)
        {
            _messages = messages;
            OtherLines = otherLines;
            _windowsPaths = windowsPaths;
        }

        /// <summary>
        /// The non-empty lines that name none of the files and do not just report one formatted (a
        /// configuration error, a crash message), without tctidy's <c>tctidy: </c> prefix.
        /// </summary>
        public IReadOnlyList<string> OtherLines { get; }

        /// <param name="stderr">Everything the run wrote to stderr.</param>
        /// <param name="paths">The paths handed to tctidy, spelled as in the list.</param>
        /// <param name="windowsPaths">Compare paths as Windows does (see the class summary).</param>
        public static BatchDiagnostics Parse(string stderr, IEnumerable<string> paths, bool windowsPaths)
        {
            if (stderr == null) throw new ArgumentNullException(nameof(stderr));
            if (paths == null) throw new ArgumentNullException(nameof(paths));

            var comparer = windowsPaths ? StringComparer.OrdinalIgnoreCase : StringComparer.Ordinal;
            var targets = new HashSet<string>(comparer);
            foreach (var path in paths) targets.Add(Key(path, windowsPaths));

            var messages = new Dictionary<string, string>(comparer);
            var other = new List<string>();
            foreach (var raw in stderr.Split('\n'))
            {
                var line = raw.TrimEnd('\r');
                if (line.Trim().Length == 0) continue;
                if (!line.StartsWith(ToolPrefix, StringComparison.Ordinal))
                {
                    other.Add(line.Trim());
                    continue;
                }

                var rest = line.Substring(ToolPrefix.Length);
                if (TryMatch(rest, targets, windowsPaths, out var key, out var message))
                {
                    // The first line about a file is its diagnostic, as tctidy reports it first.
                    if (!messages.ContainsKey(key)) messages.Add(key, message);
                }
                else if (!IsFormattedLine(rest, targets, windowsPaths))
                {
                    other.Add(rest.Trim());
                }
            }

            return new BatchDiagnostics(messages, other, windowsPaths);
        }

        /// <summary>tctidy's diagnostic for <paramref name="path"/>, without the echoed path.</summary>
        public bool TryGetMessage(string path, out string message)
        {
            if (path == null) throw new ArgumentNullException(nameof(path));
            if (_messages.TryGetValue(Key(path, _windowsPaths), out var found))
            {
                message = found;
                return true;
            }
            message = string.Empty;
            return false;
        }

        // The longest file path that `rest` starts with, followed by ": ". The separators are tried
        // from the right, so the longest candidate wins; the colon after a Windows drive letter is
        // followed by a backslash, never a space, so it is never taken for a separator.
        private static bool TryMatch(
            string rest, HashSet<string> targets, bool windowsPaths, out string key, out string message)
        {
            var at = rest.Length;
            while (at > 0 && (at = rest.LastIndexOf(Separator, at - 1, StringComparison.Ordinal)) > 0)
            {
                var candidate = Key(rest.Substring(0, at), windowsPaths);
                if (!targets.Contains(candidate)) continue;

                key = candidate;
                var text = rest.Substring(at + Separator.Length).Trim();
                message = text.Length > 0 ? text : rest.Trim();
                return true;
            }

            key = string.Empty;
            message = string.Empty;
            return false;
        }

        // tctidy's "formatted <path>" report for one of the files: neither a diagnostic nor news.
        private static bool IsFormattedLine(string rest, HashSet<string> targets, bool windowsPaths) =>
            rest.StartsWith(FormattedPrefix, StringComparison.Ordinal)
            && targets.Contains(Key(rest.Substring(FormattedPrefix.Length), windowsPaths));

        private static string Key(string path, bool windowsPaths) =>
            windowsPaths ? path.Replace('/', '\\') : path;
    }
}
