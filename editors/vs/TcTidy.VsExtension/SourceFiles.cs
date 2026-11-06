using System;
using System.Collections.Generic;
using System.IO;
using EnvDTE;
using Microsoft.VisualStudio.Shell;

namespace TcTidy.VsExtension
{
    /// <summary>
    /// Resolves the TwinCAT Structured Text files a tree-launched "Format" should act on, from the DTE
    /// selection (folders and PLC projects recurse). Each file maps to the <see cref="ProjectItem"/> that
    /// owns it, so an open POU can be formatted rooted at its own tree item rather than at whichever view
    /// is open. Uses EnvDTE rather than the toolkit's solution model because the TwinCAT hierarchy is a
    /// custom tree the toolkit does not surface as physical files. Launching from inside
    /// an element's editor is handled separately by <see cref="FormatElementCommand"/>.
    /// </summary>
    internal static class SourceFiles
    {
        private static readonly HashSet<string> Extensions =
            new HashSet<string>(StringComparer.OrdinalIgnoreCase)
            { ".TcPOU", ".TcGVL", ".TcDUT", ".TcIO" };

        public static bool IsSourceFile(string? path) =>
            !string.IsNullOrEmpty(path) && Extensions.Contains(Path.GetExtension(path));

        /// <summary>Collect the selected source files (recursing folders/projects), each mapped to its
        /// owning project item, keyed by on-disk path. UI thread only.</summary>
        public static IReadOnlyDictionary<string, ProjectItem> Resolve(DTE dte)
        {
            ThreadHelper.ThrowIfNotOnUIThread();
            var found = new Dictionary<string, ProjectItem>(StringComparer.OrdinalIgnoreCase);

            foreach (SelectedItem item in dte.SelectedItems)
            {
                if (item.ProjectItem != null) CollectItem(item.ProjectItem, found);
                else if (item.Project != null) CollectItems(item.Project.ProjectItems, found);
            }

            return found;
        }

        /// <summary>
        /// The element's name qualified by its owners up to the POU, e.g. <c>FB_X.Init</c> for a method,
        /// <c>FB_X</c> for the POU body, so a failure report names the offending element. A top-level POU
        /// backs a <c>.TcPOU</c> file; a sub-object (method/action/accessor) does not, so it is prefixed
        /// with its owner. UI thread only.
        /// </summary>
        public static string QualifiedName(ProjectItem item)
        {
            ThreadHelper.ThrowIfNotOnUIThread();
            var name = SafeName(item);
            if (HasSourceFile(item)) return name;
            var parent = ParentItem(item);
            return parent != null ? QualifiedName(parent) + "." + name : name;
        }

        /// <summary>
        /// The source file that holds the element: its own for a top-level POU, its owner's for a
        /// method/action/accessor. Null when none can be read. UI thread only.
        /// </summary>
        public static string? FilePath(ProjectItem item)
        {
            ThreadHelper.ThrowIfNotOnUIThread();
            for (var current = item; current != null; current = ParentItem(current))
            {
                var path = SourceFileOf(current);
                if (path != null) return path;
            }
            return null;
        }

        private static bool HasSourceFile(ProjectItem item) => SourceFileOf(item) != null;

        private static string? SourceFileOf(ProjectItem item)
        {
            try
            {
                for (short i = 1; i <= item.FileCount; i++)
                    if (IsSourceFile(item.FileNames[i])) return item.FileNames[i];
            }
            catch { /* some tree items have no backing file */ }
            return null;
        }

        private static ProjectItem? ParentItem(ProjectItem item)
        {
            try { return item.Collection?.Parent as ProjectItem; }
            catch { return null; }
        }

        private static string SafeName(ProjectItem item)
        {
            try { return item.Name; }
            catch { return "<item>"; }
        }

        // A source file contributes itself (mapped to its owning item); folders/projects are walked for
        // descendants. The walk never descends into a file, so a POU's methods/actions (stored in the same
        // .TcPOU) are not double-counted, and the first writer wins, so the path stays mapped to the
        // owning POU rather than to a nested method that reports the same file.
        private static void CollectItem(ProjectItem item, Dictionary<string, ProjectItem> acc)
        {
            for (short i = 1; i <= item.FileCount; i++)
            {
                string? path = null;
                try { path = item.FileNames[i]; } catch { /* some tree items have no backing file */ }
                if (IsSourceFile(path) && !acc.ContainsKey(path!)) acc[path!] = item;
            }
            CollectItems(item.ProjectItems, acc);
        }

        private static void CollectItems(ProjectItems? items, Dictionary<string, ProjectItem> acc)
        {
            if (items == null) return;
            foreach (ProjectItem child in items) CollectItem(child, acc);
        }
    }
}
