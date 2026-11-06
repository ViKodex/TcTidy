using System.Collections.Generic;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// One POU open in the editor, abstracted away from Visual Studio and COM so the format
    /// orchestration (<see cref="OpenPouFormatter"/>) can be unit-tested. The VSIX shell backs this
    /// with the TwinCAT Automation Interface; the tests back it with a fake.
    /// <para>
    /// A POU exposes one or more editable <see cref="IStSection"/>s: its own declaration and
    /// implementation, and, when formatted as a whole from the Solution Explorer, those of its
    /// methods, actions and properties too (the shell builds the list with the section walker).
    /// <see cref="Flush"/> and <see cref="Save"/> act on the whole POU: one editor document backs the
    /// single <c>.TcPOU</c> file, so saving it commits every section.
    /// </para>
    /// </summary>
    public interface IOpenPou
    {
        /// <summary>A short identifier used in failure messages (the POU name).</summary>
        string Name { get; }

        /// <summary>
        /// The POU's file on disk (its <c>.TcPOU</c>, <c>.TcGVL</c>, …), when known: its sections are
        /// formatted with the <c>.tctidy.toml</c> that applies to that file, as on disk.
        /// </summary>
        string? FilePath { get; }

        /// <summary>
        /// The editable ST sections, in a stable order. Sections the POU does not have are simply
        /// absent (a GVL/DUT lists no implementation, an Action no declaration), so the orchestrator
        /// never juggles nulls.
        /// </summary>
        IReadOnlyList<IStSection> Sections { get; }

        /// <summary>
        /// Commit the editor's dirty buffer into the model so a later <see cref="IStSection.Read"/>
        /// returns the on-screen text rather than the last-saved text (flush before read). A no-op when
        /// the document is not dirty. Throws <see cref="FormatterException"/> if the flush cannot be
        /// performed (e.g. a read-only or locked file), the orchestrator then skips and reports the
        /// POU instead of formatting the stale model.
        /// </summary>
        void Flush();

        /// <summary>Open a per-POU edit transaction before the first section write.</summary>
        void BeginEdit();

        /// <summary>Commit the current per-POU edit transaction after a successful save.</summary>
        void CommitEdit();

        /// <summary>Abort the current transaction and discard every edit made since
        /// <see cref="BeginEdit"/>.</summary>
        void AbortEdit();

        /// <summary>Persist the formatted result (the IDE-initiated save after the rewrite).</summary>
        void Save();
    }

    /// <summary>One editable Structured Text section of an <see cref="IOpenPou"/>.</summary>
    public interface IStSection
    {
        /// <summary>
        /// The element this section belongs to, qualified for diagnostics, e.g. <c>FB_X.Init</c> for a
        /// method, <c>FB_X</c> for the POU body. Used to name the failing element in a parse-error report
        /// (a method's declaration and implementation share the one name).
        /// </summary>
        string Name { get; }

        /// <summary>Which grammar tctidy should use for this section.</summary>
        StSection Kind { get; }

        /// <summary>The current text of this section.</summary>
        string Read();

        /// <summary>Replace this section's text.</summary>
        void Write(string text);

        /// <summary>Restore this section after a later write or save failed.</summary>
        void Restore(string text);
    }
}
