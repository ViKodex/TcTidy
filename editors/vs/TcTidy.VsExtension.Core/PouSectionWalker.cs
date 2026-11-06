using System;
using System.Collections.Generic;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// Flattens a POU tree into the ordered list of ST sections to format. Depth-first: a node
    /// contributes its declaration then its implementation (each only when present), then, when
    /// <paramref name="recurse"/> is set, its children in order. The two modes are the two ways
    /// "Format with TcTidy" is invoked: from the Solution Explorer on a POU, recurse into every
    /// method/action/property; from inside one element's editor, that element alone. Either
    /// way an element contributes both its declaration and its implementation, never one pane alone.
    /// Pure: no Visual Studio or COM dependency, so it is unit-tested without an IDE or a TwinCAT install.
    /// </summary>
    public static class PouSectionWalker
    {
        public static IReadOnlyList<IStSection> Collect(IPouTreeNode node, bool recurse)
        {
            if (node == null) throw new ArgumentNullException(nameof(node));

            var sections = new List<IStSection>();
            Visit(node, recurse, sections);
            return sections;
        }

        private static void Visit(IPouTreeNode node, bool recurse, List<IStSection> acc)
        {
            if (node.Declaration is { } declaration) acc.Add(declaration);
            if (node.Implementation is { } implementation) acc.Add(implementation);
            if (!recurse) return;
            foreach (var child in node.Children) Visit(child, true, acc);
        }
    }
}
