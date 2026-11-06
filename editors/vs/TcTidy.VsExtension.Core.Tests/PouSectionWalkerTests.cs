using System;
using System.Collections.Generic;
using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    /// <summary>
    /// Exercises <see cref="PouSectionWalker"/> with hand-written fake nodes (no IDE, no TwinCAT). The
    /// two modes mirror the two ways "Format with TcTidy" is invoked: from the Solution Explorer on a
    /// POU, every method/action/property is formatted (recurse); from inside one element's editor,
    /// only that element is (no recurse). Either way an element contributes BOTH its declaration
    /// and its implementation, never one pane alone.
    /// </summary>
    public sealed class PouSectionWalkerTests
    {
        [Fact]
        public void Recurse_collects_the_pou_then_each_child_depth_first()
        {
            var pouDecl = Sec(StSection.Declaration, "pou.decl");
            var pouImpl = Sec(StSection.Implementation, "pou.impl");
            var methodDecl = Sec(StSection.Declaration, "method.decl");
            var methodImpl = Sec(StSection.Implementation, "method.impl");
            var actionImpl = Sec(StSection.Implementation, "action.impl"); // action: implementation only
            var method = new FakeNode(methodDecl, methodImpl);
            var action = new FakeNode(null, actionImpl);
            var pou = new FakeNode(pouDecl, pouImpl, method, action);

            var result = PouSectionWalker.Collect(pou, recurse: true);

            Assert.Equal(
                new[] { pouDecl, pouImpl, methodDecl, methodImpl, actionImpl }, result);
        }

        [Fact]
        public void Recurse_descends_two_levels_into_a_property_get_and_set()
        {
            // A property is modelled as a node carrying the property signature declaration, with get/set
            // accessor children that each carry their own declaration + implementation. Missing this
            // second level would silently skip the accessor bodies.
            var propDecl = Sec(StSection.Declaration, "prop.decl");
            var getDecl = Sec(StSection.Declaration, "get.decl");
            var getImpl = Sec(StSection.Implementation, "get.impl");
            var setDecl = Sec(StSection.Declaration, "set.decl");
            var setImpl = Sec(StSection.Implementation, "set.impl");
            var get = new FakeNode(getDecl, getImpl);
            var set = new FakeNode(setDecl, setImpl);
            var property = new FakeNode(propDecl, null, get, set);
            var pou = new FakeNode(Sec(StSection.Declaration, "pou.decl"),
                                   Sec(StSection.Implementation, "pou.impl"), property);

            var result = PouSectionWalker.Collect(pou, recurse: true);

            Assert.Equal(7, result.Count);
            Assert.Equal(propDecl, result[2]);
            Assert.Equal(getDecl, result[3]);
            Assert.Equal(getImpl, result[4]);
            Assert.Equal(setDecl, result[5]);
            Assert.Equal(setImpl, result[6]);
        }

        [Fact]
        public void Without_recurse_only_the_node_itself_is_collected()
        {
            // The editor path: focus is on the POU body, so its methods/actions are left alone.
            var pouDecl = Sec(StSection.Declaration, "pou.decl");
            var pouImpl = Sec(StSection.Implementation, "pou.impl");
            var method = new FakeNode(Sec(StSection.Declaration, "method.decl"),
                                      Sec(StSection.Implementation, "method.impl"));
            var pou = new FakeNode(pouDecl, pouImpl, method);

            var result = PouSectionWalker.Collect(pou, recurse: false);

            Assert.Equal(new[] { pouDecl, pouImpl }, result);
        }

        [Fact]
        public void Without_recurse_a_method_contributes_its_declaration_and_implementation()
        {
            // The editor path on a method/action/accessor: format that element alone, both its panes.
            var methodDecl = Sec(StSection.Declaration, "method.decl");
            var methodImpl = Sec(StSection.Implementation, "method.impl");
            var method = new FakeNode(methodDecl, methodImpl);

            var result = PouSectionWalker.Collect(method, recurse: false);

            Assert.Equal(new[] { methodDecl, methodImpl }, result);
        }

        [Fact]
        public void A_node_without_a_declaration_contributes_only_its_implementation()
        {
            var actionImpl = Sec(StSection.Implementation, "action.impl");
            var action = new FakeNode(null, actionImpl);

            var result = PouSectionWalker.Collect(action, recurse: false);

            Assert.Equal(new[] { actionImpl }, result);
        }

        [Fact]
        public void A_node_with_no_sections_is_skipped_but_its_children_are_still_visited()
        {
            // A non-ST body surfaces as a node with no declaration and no implementation; it contributes
            // nothing, yet a real sibling, and any children, are still formatted.
            var nonSt = new FakeNode(null, null);
            var method = new FakeNode(Sec(StSection.Declaration, "method.decl"),
                                      Sec(StSection.Implementation, "method.impl"));
            var pou = new FakeNode(Sec(StSection.Declaration, "pou.decl"),
                                   Sec(StSection.Implementation, "pou.impl"), nonSt, method);

            var result = PouSectionWalker.Collect(pou, recurse: true);

            Assert.Equal(4, result.Count); // non-ST node added nothing
        }

        [Fact]
        public void A_container_with_no_own_sections_still_yields_its_children()
        {
            var childImpl = Sec(StSection.Implementation, "child.impl");
            var container = new FakeNode(null, null, new FakeNode(null, childImpl));

            var result = PouSectionWalker.Collect(container, recurse: true);

            Assert.Equal(new[] { childImpl }, result);
        }

        [Fact]
        public void An_empty_node_yields_no_sections_in_either_mode()
        {
            var node = new FakeNode();

            Assert.Empty(PouSectionWalker.Collect(node, recurse: true));
            Assert.Empty(PouSectionWalker.Collect(node, recurse: false));
        }

        [Fact]
        public void A_null_node_is_rejected()
        {
            Assert.Throws<ArgumentNullException>(() => PouSectionWalker.Collect(null!, recurse: true));
        }

        private static IStSection Sec(StSection kind, string tag) => new FakeSection(kind, tag);

        // ----- fakes -----

        // A minimal section identified by a tag, compared by reference in the assertions above.
        private sealed class FakeSection : IStSection
        {
            private readonly string _tag;

            public FakeSection(StSection kind, string tag)
            {
                Kind = kind;
                _tag = tag;
            }

            public string Name => _tag;
            public StSection Kind { get; }
            public string Read() => _tag;
            public void Write(string text) { }
            public void Restore(string text) { }
            public override string ToString() => _tag;
        }

        private sealed class FakeNode : IPouTreeNode
        {
            public FakeNode(
                IStSection? declaration = null, IStSection? implementation = null,
                params IPouTreeNode[] children)
            {
                Declaration = declaration;
                Implementation = implementation;
                Children = children;
            }

            public IStSection? Declaration { get; }
            public IStSection? Implementation { get; }
            public IReadOnlyList<IPouTreeNode> Children { get; }
        }
    }
}
