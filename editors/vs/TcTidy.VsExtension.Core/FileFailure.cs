using System;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// One file (or open-POU element) that tctidy refused or could not format, left untouched. Carries
    /// the identifier and the diagnostic separately so a report can surface each failure individually:
    /// the on-disk path keeps its full path (not just the file name, so identically named POUs in
    /// different folders stay distinguishable), while an in-memory POU has no disk path and carries its
    /// qualified element name instead (e.g. <c>FB_X.Init</c>).
    /// </summary>
    public sealed class FileFailure
    {
        /// <summary>Full disk path (disk batch) or qualified element name (open POU).</summary>
        public string Source { get; }

        /// <summary>tctidy's diagnostic for this file, already de-prefixed by the runner.</summary>
        public string Message { get; }

        public FileFailure(string source, string message)
        {
            Source = source ?? throw new ArgumentNullException(nameof(source));
            Message = message ?? throw new ArgumentNullException(nameof(message));
        }

        /// <summary>The natural one-line rendering, "<c>source: message</c>", the form callers used
        /// when failures were plain strings. Multi-line layout is the report renderer's job, not this.</summary>
        public override string ToString() => Source + ": " + Message;
    }
}
