using Community.VisualStudio.Toolkit;
using Microsoft.VisualStudio;
using Microsoft.VisualStudio.Shell;
using Microsoft.VisualStudio.Shell.Interop;
using Task = System.Threading.Tasks.Task;

namespace TcTidy.VsExtension
{
    /// <summary>
    /// The single, cached "TcTidy" Output window pane, shared by the run report (<see cref="FormatSupport"/>)
    /// and the perf sink (<see cref="PerfMode"/>) so both write to one pane. Every call is best-effort: a
    /// diagnostic surface must never break a format, so a misbehaving Output API is swallowed.
    /// </summary>
    internal static class TcTidyOutput
    {
        private static OutputWindowPane? _pane;

        /// <summary>Append a line to the pane (creating it on first use). Does not steal focus.</summary>
        public static async Task WriteLineAsync(string text)
        {
            try
            {
                _pane ??= await VS.Windows.CreateOutputWindowPaneAsync("TcTidy");
                await _pane.WriteLineAsync(text);
            }
            catch
            {
                // best effort, a diagnostic never breaks a format
            }
        }

        /// <summary>Bring the Output window to the front and select the "TcTidy" pane (used only when
        /// there is something the user must see). Showing the tool-window frame is the part that actually
        /// surfaces it: activating the pane alone selects the "TcTidy" entry inside the Output window but
        /// leaves that window behind whatever sibling tab it is docked with (e.g. the Error List), so the frame is
        /// shown first (<see cref="IVsWindowFrame.Show"/>), then the pane is activated.</summary>
        public static async Task ActivateAsync()
        {
            try
            {
                await ThreadHelper.JoinableTaskFactory.SwitchToMainThreadAsync();

                var shell = await VS.GetRequiredServiceAsync<SVsUIShell, IVsUIShell>();
                var outputWindow = VSConstants.StandardToolWindows.Output;
                if (ErrorHandler.Succeeded(shell.FindToolWindow(0, ref outputWindow, out var frame))
                    && frame != null)
                {
                    frame.Show();
                }

                _pane ??= await VS.Windows.CreateOutputWindowPaneAsync("TcTidy");
                await _pane.ActivateAsync();
            }
            catch
            {
                // best effort
            }
        }
    }
}
