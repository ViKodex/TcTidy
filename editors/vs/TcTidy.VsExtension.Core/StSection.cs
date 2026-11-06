namespace TcTidy.VsExtension.Core
{
    /// <summary>Which ST grammar tctidy should use for a raw text fragment.</summary>
    public enum StSection
    {
        /// <summary>Let tctidy decide (it defaults to the implementation grammar).</summary>
        Auto,

        /// <summary>A <c>&lt;Declaration&gt;</c> section.</summary>
        Declaration,

        /// <summary>An <c>&lt;Implementation&gt;</c> body.</summary>
        Implementation,
    }
}
