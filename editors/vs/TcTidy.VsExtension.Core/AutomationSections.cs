using System;
using System.Globalization;
using System.Reflection;
using System.Runtime.InteropServices;

namespace TcTidy.VsExtension.Core
{
    /// <summary>IEC implementation languages exposed by TwinCAT's Automation Interface.</summary>
    public enum PlcImplementationLanguage
    {
        None = 0,
        StructuredText = 1,
        InstructionList = 2,
        SequentialFunctionChart = 3,
        FunctionBlockDiagram = 4,
        ContinuousFunctionChart = 5,
        LadderDiagram = 6,
    }

    /// <summary>
    /// Discovers editable text areas on a late-bound TwinCAT PLC tree item. Declarations are
    /// language-independent. An implementation is exposed only when its <c>Language</c> property says
    /// Structured Text, and that property is read before <c>ImplementationText</c>. Declaration-only
    /// adapters such as GVLs may omit <c>Language</c> entirely, and implementation-only adapters
    /// (Actions, Transitions) have no <c>DeclarationText</c> member at all.
    /// </summary>
    public sealed class AutomationSections
    {
        private readonly object _treeItem;
        private readonly string _name;

        public AutomationSections(object treeItem, string name)
        {
            _treeItem = treeItem ?? throw new ArgumentNullException(nameof(treeItem));
            _name = name ?? throw new ArgumentNullException(nameof(name));
        }

        public IStSection? Declaration =>
            ReadTextSection("DeclarationText", StSection.Declaration, "declaration");

        public IStSection? Implementation
        {
            get
            {
                if (ReadImplementationLanguage() != PlcImplementationLanguage.StructuredText)
                    return null;
                return ReadTextSection(
                    "ImplementationText", StSection.Implementation, "implementation");
            }
        }

        private PlcImplementationLanguage ReadImplementationLanguage()
        {
            try
            {
                if (!AutomationProperty.TryGet(_treeItem, "Language", out var value))
                    return PlcImplementationLanguage.None;
                value = value
                    ?? throw new InvalidOperationException("Language returned no value");
                return (PlcImplementationLanguage)Convert.ToInt32(
                    value, CultureInfo.InvariantCulture);
            }
            catch (Exception ex)
            {
                throw new FormatterException(
                    -1,
                    $"could not read {_name} implementation language (Language): "
                    + AutomationProperty.ErrorMessage(ex));
            }
        }

        private IStSection? ReadTextSection(string property, StSection kind, string description)
        {
            try
            {
                // A missing member means the item has no such area, like a null value does.
                if (!AutomationProperty.TryGet(_treeItem, property, out var text)) return null;
                if (text == null) return null;
                if (!(text is string source))
                    throw new InvalidOperationException(property + " did not return text");
                return new AutomationTextSection(_treeItem, property, kind, _name, source);
            }
            catch (Exception ex)
            {
                throw new FormatterException(
                    -1,
                    $"could not read {_name} {description} ({property}): "
                    + AutomationProperty.ErrorMessage(ex));
            }
        }

        private sealed class AutomationTextSection : IStSection
        {
            private readonly object _treeItem;
            private readonly string _property;
            private string? _initialText;

            public AutomationTextSection(
                object treeItem, string property, StSection kind, string name, string initialText)
            {
                _treeItem = treeItem;
                _property = property;
                _initialText = initialText;
                Kind = kind;
                Name = name;
            }

            public string Name { get; }
            public StSection Kind { get; }

            public string Read()
            {
                if (_initialText != null)
                {
                    var text = _initialText;
                    _initialText = null;
                    return text;
                }

                try
                {
                    var value = AutomationProperty.Get(_treeItem, _property)
                        ?? throw new InvalidOperationException(
                            _property + " returned no value after discovery");
                    return value as string
                        ?? throw new InvalidOperationException(_property + " did not return text");
                }
                catch (Exception ex)
                {
                    throw new FormatterException(-1, AutomationProperty.ErrorMessage(ex));
                }
            }

            public void Write(string text) => SetText(text);

            public void Restore(string text) => SetText(text);

            private void SetText(string text)
            {
                try { AutomationProperty.Set(_treeItem, _property, text); }
                catch (Exception ex)
                {
                    throw new FormatterException(-1, AutomationProperty.ErrorMessage(ex));
                }
            }
        }

        private static class AutomationProperty
        {
            public static object? Get(object item, string property) =>
                item.GetType().InvokeMember(
                    property, BindingFlags.GetProperty, null, item, null);

            public static bool TryGet(object item, string property, out object? value)
            {
                try
                {
                    value = Get(item, property);
                    return true;
                }
                catch (MissingMemberException)
                {
                    value = null;
                    return false;
                }
                // DISP_E_MEMBERNOTFOUND or DISP_E_UNKNOWNNAME: a COM object without the member.
                catch (COMException ex) when (
                    ex.ErrorCode == unchecked((int)0x80020003)
                    || ex.ErrorCode == unchecked((int)0x80020006))
                {
                    value = null;
                    return false;
                }
            }

            public static void Set(object item, string property, string value) =>
                item.GetType().InvokeMember(
                    property, BindingFlags.SetProperty, null, item, new object[] { value });

            public static string ErrorMessage(Exception error)
            {
                while (error is TargetInvocationException && error.InnerException != null)
                    error = error.InnerException;
                return error.Message;
            }
        }
    }
}
