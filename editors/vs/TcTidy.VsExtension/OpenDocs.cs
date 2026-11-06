using System;
using System.Collections.Generic;
using System.Reflection;
using EnvDTE;
using Microsoft.VisualStudio.Shell;
using TcTidy.VsExtension.Core;

namespace TcTidy.VsExtension
{
    /// <summary>
    /// A TwinCAT PLC tree item that can be formatted in place, the COM-backed <see cref="IOpenPou"/>. Its ST
    /// sections are read and written through the TwinCAT Automation Interface (late-bound
    /// <c>DeclarationText</c>/<c>ImplementationText</c>), and the EnvDTE <see cref="Document"/> backs the
    /// flush and the save.
    /// <para>
    /// When <c>recurse</c> is set (a POU selected in the Solution Explorer) the section list spans the
    /// whole POU, its body plus every method/action/property and their accessors; otherwise (the
    /// element open in the editor) it is just that element's own declaration and implementation.
    /// Accessed late-bound so no <c>TCatSysManagerLib</c> reference is needed at build time, the
    /// <c>.vsix</c> builds without TwinCAT installed.
    /// </para>
    /// </summary>
    internal sealed class OpenItem : IOpenPou
    {
        private readonly ProjectItem _projectItem;
        private readonly object _treeItem;
        private readonly Document _document;
        private readonly DTE _dte;
        private readonly bool _recurse;
        private IReadOnlyList<IStSection>? _sections;
        private bool _ownsUndo;

        public OpenItem(
            ProjectItem projectItem, object treeItem, Document document, DTE dte, bool recurse, string name,
            string? filePath)
        {
            _projectItem = projectItem;
            _treeItem = treeItem;
            _document = document;
            _dte = dte;
            _recurse = recurse;
            Name = name;
            FilePath = filePath;
        }

        /// <summary>The element's qualified name (e.g. <c>FB_X.Init</c>), used in failure reports.</summary>
        public string Name { get; }

        /// <summary>The POU's source file, so its sections get that file's <c>.tctidy.toml</c>.</summary>
        public string? FilePath { get; }

        // Built lazily on first read, which OpenPouFormatter does AFTER Flush(), so the walk reflects
        // the flushed (committed) tree. The flat, ordered section list is produced by the tested Core
        // walker; this adapter only supplies the COM-backed tree node, rooted at this POU's name.
        public IReadOnlyList<IStSection> Sections =>
            _sections ??= PouSectionWalker.Collect(new TreeItemNode(_projectItem, _treeItem, Name), _recurse);

        // Commit the editor's dirty buffer into the model so the section reads return the on-screen text,
        // not the last-saved text. An IDE-initiated save is not an "external" change, so it
        // raises no "changed outside the environment" reload prompt; saving only when the document is
        // dirty leaves a clean POU untouched. Unlike Save() this must NOT swallow: a flush that cannot
        // commit is surfaced as a failure so the orchestrator skips the POU rather than formatting (and
        // then persisting) the stale model.
        public void Flush()
        {
            ThreadHelper.ThrowIfNotOnUIThread();
            try
            {
                if (!_document.Saved) _document.Save();
            }
            catch (Exception ex)
            {
                throw new FormatterException(
                    -1, $"could not save before formatting: {Com.ErrorMessage(ex)}");
            }
        }

        public void BeginEdit()
        {
            ThreadHelper.ThrowIfNotOnUIThread();
            try
            {
                var undo = _dte.UndoContext;
                if (undo == null) throw new InvalidOperationException("UndoContext is unavailable");
                if (undo.IsOpen) throw new InvalidOperationException("another undo transaction is open");
                undo.Open("Format with TcTidy: " + Name, false);
                _ownsUndo = true;
            }
            catch (Exception ex)
            {
                throw new FormatterException(-1, Com.ErrorMessage(ex));
            }
        }

        public void CommitEdit()
        {
            ThreadHelper.ThrowIfNotOnUIThread();
            if (!_ownsUndo) return;
            try
            {
                _dte.UndoContext.Close();
                _ownsUndo = false;
            }
            catch (Exception ex)
            {
                throw new FormatterException(-1, Com.ErrorMessage(ex));
            }
        }

        public void AbortEdit()
        {
            ThreadHelper.ThrowIfNotOnUIThread();
            if (!_ownsUndo) return;

            Exception? failure = null;
            try { _dte.UndoContext.SetAborted(); }
            catch (Exception ex) { failure = ex; }
            try
            {
                if (_dte.UndoContext.IsOpen) _dte.UndoContext.Close();
                _ownsUndo = false;
            }
            catch (Exception ex)
            {
                if (failure == null) failure = ex;
            }

            if (failure != null)
                throw new FormatterException(-1, Com.ErrorMessage(failure));
        }

        // Persist the formatted result by saving the editor DOCUMENT (equivalent to Ctrl+S on the tab),
        // which writes the whole .TcPOU file, including sub-objects whose ProjectItem.Save() is a no-op.
        // Never swallow: persistence is part of the transaction's success contract.
        public void Save()
        {
            ThreadHelper.ThrowIfNotOnUIThread();
            try { _document.Save(); }
            catch (Exception ex)
            {
                throw new FormatterException(-1, Com.ErrorMessage(ex));
            }
        }
    }

    /// <summary>
    /// One node of a POU's tree, the COM-backed <see cref="IPouTreeNode"/>. A node is the POU itself or
    /// any of its methods/actions/transitions/properties (and a property's get/set accessors). Its text
    /// areas are read through the Automation Interface tree item (<c>ProjectItem.Object</c>). The
    /// implementation's <c>Language</c> is checked before its text is read, while declarations remain
    /// independent of that language. A missing or null text property means the node has no such area
    /// (an Action and a Transition have no <c>DeclarationText</c> member). Children are the nested EnvDTE project items, each carrying
    /// its own tree item; this is how <see cref="SourceFiles"/> already walks the POU tree, and it is
    /// more reliable here than the COM <c>ChildCount</c>/<c>Child</c> indexer (which
    /// <c>ProjectItem.Object</c> does not reliably expose).
    /// </summary>
    internal sealed class TreeItemNode : IPouTreeNode
    {
        private readonly ProjectItem _item;
        private readonly AutomationSections _sections;
        private readonly string _name;

        public TreeItemNode(ProjectItem item, object treeItem, string name)
        {
            _item = item;
            _sections = new AutomationSections(treeItem, name);
            _name = name;
        }

        public IStSection? Declaration => _sections.Declaration;

        public IStSection? Implementation => _sections.Implementation;

        public IReadOnlyList<IPouTreeNode> Children
        {
            get
            {
                var children = new List<IPouTreeNode>();

                ProjectItems items;
                try { items = _item.ProjectItems; }
                catch { return children; }
                if (items == null) return children;

                foreach (ProjectItem child in items)
                {
                    object treeItem;
                    string childName;
                    try
                    {
                        treeItem = child.Object;
                        childName = child.Name;
                    }
                    catch { continue; }
                    if (treeItem != null)
                        children.Add(new TreeItemNode(child, treeItem, _name + "." + childName));
                }
                return children;
            }
        }
    }

    /// <summary>Extracts the underlying message from a late-bound COM invocation error.</summary>
    internal static class Com
    {
        public static string ErrorMessage(Exception error)
        {
            while (error is TargetInvocationException && error.InnerException != null)
                error = error.InnerException;
            return error.Message;
        }
    }

    /// <summary>Discovers which TwinCAT source files are open in the editor.</summary>
    internal static class OpenDocuments
    {
        /// <summary>
        /// Map each open source-file tab to its EnvDTE <see cref="Document"/>, keyed by on-disk path. UI
        /// thread only. The caller uses this both to tell an open file apart from a closed one and to get
        /// a document to flush; the tree item to format is rooted at the selected POU, not at whichever
        /// view happens to be open, so a POU stays formatted as a whole even if only a method tab is open.
        /// </summary>
        public static Dictionary<string, Document> Resolve(DTE dte)
        {
            ThreadHelper.ThrowIfNotOnUIThread();
            var map = new Dictionary<string, Document>(StringComparer.OrdinalIgnoreCase);

            Documents docs;
            try { docs = dte.Documents; }
            catch { return map; }

            foreach (Document doc in docs)
            {
                string? path;
                try { path = doc.FullName; }
                catch { continue; }
                if (!SourceFiles.IsSourceFile(path)) continue;

                // First open view of the file backs the flush; any view of the one .TcPOU saves it all.
                if (!map.ContainsKey(path!)) map[path!] = doc;
            }

            return map;
        }
    }

    /// <summary>
    /// Formats open POUs in place through the TwinCAT Automation Interface. The read/format/write/count
    /// orchestration, including the flush before the read, lives in the tested Core
    /// (<see cref="OpenPouFormatter"/>), including one abortable undo transaction per POU. Because each
    /// edit goes through the IDE's object model (not the file on disk),
    /// the open editor updates with no "changed outside the environment" reload prompt.
    /// </summary>
    internal static class OpenDocFormatter
    {
        public static BatchOutcome Format(DTE dte, IReadOnlyList<OpenItem> items, ITctidyRunner runner)
        {
            ThreadHelper.ThrowIfNotOnUIThread();
            if (items.Count == 0) return new BatchOutcome(0, 0, Array.Empty<FileFailure>());

            if (dte == null) throw new ArgumentNullException(nameof(dte));
            return new OpenPouFormatter(runner).Format(items);
        }
    }
}
