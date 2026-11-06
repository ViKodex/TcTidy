namespace TcTidy.VsExtension.Core
{
    /// <summary>Outcome of formatting a file in place.</summary>
    public sealed class FormatResult
    {
        /// <summary>Whether the file's bytes changed.</summary>
        public bool Changed { get; }

        public FormatResult(bool changed) => Changed = changed;
    }
}
