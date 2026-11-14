using System;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// What tctidy made of one <see cref="SectionRequest"/>: the formatted text, or the diagnostic of
    /// a section it refused, in which case the caller keeps the original text.
    /// </summary>
    public sealed class SectionResult
    {
        private SectionResult(string? output, string? error)
        {
            Output = output;
            Error = error;
        }

        /// <summary>The formatted text; null when the section was refused.</summary>
        public string? Output { get; }

        /// <summary>tctidy's diagnostic for a refused section; null when it was formatted.</summary>
        public string? Error { get; }

        public static SectionResult Formatted(string output) =>
            new SectionResult(output ?? throw new ArgumentNullException(nameof(output)), null);

        public static SectionResult Refused(string error) =>
            new SectionResult(null, error ?? throw new ArgumentNullException(nameof(error)));
    }
}
