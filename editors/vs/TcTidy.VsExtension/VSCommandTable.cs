namespace TcTidy.VsExtension
{
    /// <summary>
    /// GUIDs and command IDs mirroring <c>VSCommandTable.vsct</c>. Kept by hand
    /// (no design-time generator runs in a command-line / CI build), every value
    /// here must match the corresponding symbol in the .vsct.
    /// </summary>
    internal static class PackageGuids
    {
        public const string guidTcTidyPackageString = "279abe1e-4a2a-4ad6-a688-0751c8f9af07";
        public const string guidTcTidyCmdSetString = "bde8f5ed-171f-46dc-903f-69c1242719c3";
    }

    internal static class PackageIds
    {
        public const int TcTidyContextGroup = 0x0001;
        public const int TcTidyEditorGroup = 0x0002;
        public const int FormatPouCommandId = 0x0100;
        public const int FormatElementCommandId = 0x0101;
        public const int DocumentationCommandId = 0x0102;
        public const int ReportBugCommandId = 0x0103;
        public const int AboutCommandId = 0x0104;
        public const int TogglePerfReportCommandId = 0x0105;
        public const int FormattingSettingsCommandId = 0x0106;
        public const int CheckForUpdatesCommandId = 0x0107;
        public const int ToggleUpdateCheckCommandId = 0x0108;
        public const int TcTidyMenu = 0x1000;
        public const int TcTidyMenuLinksGroup = 0x1100;
        public const int TcTidyMenuOptionsGroup = 0x1200;
        public const int TcTidyMenuAboutGroup = 0x1300;
    }
}
