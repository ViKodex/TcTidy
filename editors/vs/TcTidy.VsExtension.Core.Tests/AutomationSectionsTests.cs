using System;
using System.Collections.Generic;
using System.Linq;
using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    public sealed class AutomationSectionsTests
    {
        [Theory]
        [InlineData(PlcImplementationLanguage.None, false)]
        [InlineData(PlcImplementationLanguage.StructuredText, true)]
        [InlineData(PlcImplementationLanguage.InstructionList, false)]
        [InlineData(PlcImplementationLanguage.SequentialFunctionChart, false)]
        [InlineData(PlcImplementationLanguage.FunctionBlockDiagram, false)]
        [InlineData(PlcImplementationLanguage.ContinuousFunctionChart, false)]
        [InlineData(PlcImplementationLanguage.LadderDiagram, false)]
        [InlineData((PlcImplementationLanguage)99, false)]
        public void Reads_language_before_exposing_only_structured_text_implementations(
            PlcImplementationLanguage language, bool expected)
        {
            var item = new FakeTreeItem(language);
            var sections = new AutomationSections(item, "FB_X");

            var implementation = sections.Implementation;

            Assert.Equal(expected, implementation != null);
            Assert.Equal(
                expected ? new[] { "Language", "ImplementationText" } : new[] { "Language" },
                item.Accesses);
        }

        [Theory]
        [InlineData(PlcImplementationLanguage.None)]
        [InlineData(PlcImplementationLanguage.InstructionList)]
        [InlineData(PlcImplementationLanguage.SequentialFunctionChart)]
        [InlineData(PlcImplementationLanguage.FunctionBlockDiagram)]
        [InlineData(PlcImplementationLanguage.ContinuousFunctionChart)]
        [InlineData(PlcImplementationLanguage.LadderDiagram)]
        public void Keeps_declarations_eligible_for_every_non_st_implementation(
            PlcImplementationLanguage language)
        {
            var item = new FakeTreeItem(language);
            var sections = new AutomationSections(item, "FB_X");

            var declaration = sections.Declaration;
            var implementation = sections.Implementation;

            Assert.NotNull(declaration);
            Assert.Null(implementation);
            Assert.Equal(new[] { "DeclarationText", "Language" }, item.Accesses);
        }

        [Fact]
        public void Missing_language_property_is_treated_as_a_declaration_only_item()
        {
            var item = new DeclarationOnlyTreeItem();
            var sections = new AutomationSections(item, "GVL");

            var declaration = sections.Declaration;
            var implementation = sections.Implementation;

            Assert.NotNull(declaration);
            Assert.Null(implementation);
            Assert.Equal(new[] { "DeclarationText" }, item.Accesses);
        }

        [Theory]
        [InlineData(PlcImplementationLanguage.StructuredText, true)]
        [InlineData(PlcImplementationLanguage.SequentialFunctionChart, false)]
        public void Missing_declaration_property_is_treated_as_an_implementation_only_item(
            PlcImplementationLanguage language, bool expected)
        {
            var item = new ImplementationOnlyTreeItem(language);
            var sections = new AutomationSections(item, "FB_X.Start");

            var declaration = sections.Declaration;
            var implementation = sections.Implementation;

            Assert.Null(declaration);
            Assert.Equal(expected, implementation != null);
            Assert.Equal(
                expected ? new[] { "Language", "ImplementationText" } : new[] { "Language" },
                item.Accesses);
        }

        [Fact]
        public void Null_declaration_text_is_treated_as_an_absent_declaration()
        {
            var item = new FakeTreeItem(PlcImplementationLanguage.StructuredText, hasDeclaration: false);
            var sections = new AutomationSections(item, "FB_X.Start");

            Assert.Null(sections.Declaration);
            Assert.Equal(new[] { "DeclarationText" }, item.Accesses);
        }

        [Fact]
        public void Declaration_read_failure_is_still_reported()
        {
            var item = new FakeTreeItem(PlcImplementationLanguage.StructuredText)
            {
                DeclarationError = new InvalidOperationException("COM declaration boom"),
            };
            var sections = new AutomationSections(item, "FB_X");

            var error = Assert.Throws<FormatterException>(() => sections.Declaration);

            Assert.Equal(
                "could not read FB_X declaration (DeclarationText): COM declaration boom",
                error.Message);
        }

        [Theory]
        [InlineData("FB_X")]
        [InlineData("FB_X.Init")]
        [InlineData("FB_X.Start")]
        [InlineData("FB_X.Ready")]
        [InlineData("FB_X.Value")]
        [InlineData("FB_X.Value.Get")]
        [InlineData("FB_X.Value.Set")]
        public void Language_read_failure_has_item_section_and_property_context_without_text_fallback(
            string name)
        {
            var item = new FakeTreeItem(PlcImplementationLanguage.StructuredText)
            {
                LanguageError = new InvalidOperationException("COM language boom"),
            };
            var sections = new AutomationSections(item, name);

            var error = Assert.Throws<FormatterException>(() => sections.Implementation);

            Assert.Equal(
                $"could not read {name} implementation language (Language): COM language boom",
                error.Message);
            Assert.Equal(new[] { "Language" }, item.Accesses);
        }

        [Fact]
        public void Structured_text_section_uses_cached_discovery_text_then_late_bound_access()
        {
            var item = new FakeTreeItem(PlcImplementationLanguage.StructuredText);
            var section = Assert.IsAssignableFrom<IStSection>(
                new AutomationSections(item, "FB_X").Implementation);

            Assert.Equal("x:=1;", section.Read());
            Assert.Equal(
                new[] { "Language", "ImplementationText" },
                item.Accesses);

            item.ImplementationText = "y:=2;";
            Assert.Equal("y:=2;", section.Read());
            section.Write("z := 3;");
            section.Restore("x:=1;");

            Assert.Equal("x:=1;", item.ImplementationText);
            Assert.Equal(
                new[]
                {
                    "Language",
                    "ImplementationText",
                    "ImplementationText=set",
                    "ImplementationText",
                    "ImplementationText=set",
                    "ImplementationText=set",
                    "ImplementationText",
                },
                item.Accesses);
        }

        [Fact]
        public void Walker_keeps_declarations_and_only_st_bodies_across_every_pou_item_kind()
        {
            var method = Node("FB_X.Init", PlcImplementationLanguage.InstructionList);
            var action = Node(
                "FB_X.Start", PlcImplementationLanguage.SequentialFunctionChart,
                hasDeclaration: false);
            var transition = Node(
                "FB_X.Ready", PlcImplementationLanguage.StructuredText,
                hasDeclaration: false);
            var getter = Node("FB_X.Value.Get", PlcImplementationLanguage.StructuredText);
            var setter = Node("FB_X.Value.Set", PlcImplementationLanguage.LadderDiagram);
            var property = Node(
                "FB_X.Value", PlcImplementationLanguage.FunctionBlockDiagram,
                true, getter, setter);
            var pou = Node(
                "FB_X", PlcImplementationLanguage.StructuredText,
                true, method, action, transition, property);

            var sections = PouSectionWalker.Collect(pou, recurse: true);

            Assert.Equal(
                new[]
                {
                    ("FB_X", StSection.Declaration),
                    ("FB_X", StSection.Implementation),
                    ("FB_X.Init", StSection.Declaration),
                    ("FB_X.Ready", StSection.Implementation),
                    ("FB_X.Value", StSection.Declaration),
                    ("FB_X.Value.Get", StSection.Declaration),
                    ("FB_X.Value.Get", StSection.Implementation),
                    ("FB_X.Value.Set", StSection.Declaration),
                },
                sections.Select(section => (section.Name, section.Kind)));
        }

        private static AutomationNode Node(
            string name,
            PlcImplementationLanguage language,
            bool hasDeclaration = true,
            params AutomationNode[] children)
        {
            // Actions and Transitions have no DeclarationText member, as in TwinCAT's adapter.
            object item = hasDeclaration
                ? new FakeTreeItem(language)
                : new ImplementationOnlyTreeItem(language);
            return new AutomationNode(new AutomationSections(item, name), children);
        }

        private sealed class AutomationNode : IPouTreeNode
        {
            private readonly AutomationSections _sections;

            public AutomationNode(AutomationSections sections, IReadOnlyList<IPouTreeNode> children)
            {
                _sections = sections;
                Children = children;
            }

            public IStSection? Declaration => _sections.Declaration;
            public IStSection? Implementation => _sections.Implementation;
            public IReadOnlyList<IPouTreeNode> Children { get; }
        }

        public sealed class DeclarationOnlyTreeItem
        {
            public List<string> Accesses { get; } = new List<string>();

            public string DeclarationText
            {
                get
                {
                    Accesses.Add(nameof(DeclarationText));
                    return "VAR_GLOBAL\r\nx : BOOL;\r\nEND_VAR";
                }
            }
        }

        public sealed class ImplementationOnlyTreeItem
        {
            private readonly PlcImplementationLanguage _language;

            public ImplementationOnlyTreeItem(PlcImplementationLanguage language)
            {
                _language = language;
            }

            public List<string> Accesses { get; } = new List<string>();

            public PlcImplementationLanguage Language
            {
                get
                {
                    Accesses.Add(nameof(Language));
                    return _language;
                }
            }

            public string ImplementationText
            {
                get
                {
                    Accesses.Add(nameof(ImplementationText));
                    return "x:=1;";
                }
            }
        }

        public sealed class FakeTreeItem
        {
            private readonly PlcImplementationLanguage _language;
            private string? _declarationText;
            private string _implementationText = "x:=1;";

            public FakeTreeItem(PlcImplementationLanguage language, bool hasDeclaration = true)
            {
                _language = language;
                if (hasDeclaration) _declarationText = "VAR\r\nx : INT;\r\nEND_VAR";
            }

            public List<string> Accesses { get; } = new List<string>();
            public Exception? LanguageError { get; set; }
            public Exception? DeclarationError { get; set; }

            public PlcImplementationLanguage Language
            {
                get
                {
                    Accesses.Add(nameof(Language));
                    if (LanguageError != null) throw LanguageError;
                    return _language;
                }
            }

            public string? DeclarationText
            {
                get
                {
                    Accesses.Add(nameof(DeclarationText));
                    if (DeclarationError != null) throw DeclarationError;
                    return _declarationText;
                }
                set
                {
                    Accesses.Add(nameof(DeclarationText) + "=set");
                    _declarationText = value;
                }
            }

            public string ImplementationText
            {
                get
                {
                    Accesses.Add(nameof(ImplementationText));
                    return _implementationText;
                }
                set
                {
                    Accesses.Add(nameof(ImplementationText) + "=set");
                    _implementationText = value;
                }
            }
        }
    }
}
