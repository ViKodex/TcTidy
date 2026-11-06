using System.Collections.Generic;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// One node of an open POU's tree, abstracted away from Visual Studio and COM so the section walk
    /// (<see cref="PouSectionWalker"/>) can be unit-tested. The POU itself and each of its
    /// methods/actions/transitions/properties (and a property's get/set accessors) is a node. The VSIX
    /// shell backs this with the TwinCAT Automation Interface tree item; the tests back it with a fake.
    /// </summary>
    public interface IPouTreeNode
    {
        /// <summary>
        /// The node's declaration section, or <c>null</c> when it has none, an Action and a Transition
        /// have only an implementation. Null rather than empty so the walker emits nothing for it.
        /// </summary>
        IStSection? Declaration { get; }

        /// <summary>
        /// The node's implementation section, or <c>null</c> when it has none or its Automation
        /// Interface language is not Structured Text. Non-ST bodies are left untouched just as the
        /// on-disk path leaves non-<c>&lt;ST&gt;</c> bodies untouched.
        /// </summary>
        IStSection? Implementation { get; }

        /// <summary>The node's direct child nodes, in a stable order; empty when it has none.</summary>
        IReadOnlyList<IPouTreeNode> Children { get; }
    }
}
