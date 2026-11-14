using System;

namespace TcTidy.VsExtension.Core
{
    /// <summary>One raw ST section to format in a <see cref="ITctidyRunner.FormatSections"/> run.</summary>
    public sealed class SectionRequest
    {
        public SectionRequest(string source, StSection kind, string? sourcePath = null)
        {
            Source = source ?? throw new ArgumentNullException(nameof(source));
            Kind = kind;
            SourcePath = sourcePath;
        }

        /// <summary>The section's text.</summary>
        public string Source { get; }

        /// <summary>Which grammar tctidy should use for it.</summary>
        public StSection Kind { get; }

        /// <summary>
        /// The file the text comes from, when known. tctidy then formats it with the
        /// <c>.tctidy.toml</c> that applies to that file, exactly as it would format the file on disk.
        /// </summary>
        public string? SourcePath { get; }
    }
}
