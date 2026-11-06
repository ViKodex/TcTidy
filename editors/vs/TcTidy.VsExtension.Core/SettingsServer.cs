using System;
using System.Diagnostics;
using System.Text;
using System.Threading.Tasks;

namespace TcTidy.VsExtension.Core
{
    /// <summary>A running settings page server, as <see cref="SettingsPageHost"/> sees it.</summary>
    public interface ISettingsServer : IDisposable
    {
        /// <summary>The page to open, including its one-time token.</summary>
        string Url { get; }

        /// <summary>The directory whose <c>.tctidy.toml</c> the page edits.</summary>
        string RootDirectory { get; }

        /// <summary>Whether the server still serves (it stops on its own once the page is closed).</summary>
        bool IsRunning { get; }
    }

    /// <summary>
    /// A running <c>tctidy --settings</c>: the local settings page for the <c>.tctidy.toml</c> that
    /// applies to a directory. tctidy prints the page's URL (loopback address plus a one-time token)
    /// as its first stdout line and serves until the page has been closed for a while, so the caller
    /// only opens <see cref="Url"/> in a browser. No Visual Studio dependency.
    /// </summary>
    public sealed class SettingsServer : ISettingsServer
    {
        private const string UrlPrefix = "http://127.0.0.1:";

        private readonly Process _process;

        private SettingsServer(Process process, string url, string rootDirectory)
        {
            _process = process;
            Url = url;
            RootDirectory = rootDirectory;
        }

        public string Url { get; }

        public string RootDirectory { get; }

        public bool IsRunning
        {
            get
            {
                try { return !_process.HasExited; }
                catch (InvalidOperationException) { return false; }
            }
        }

        /// <summary>
        /// Start the settings server for <paramref name="rootDirectory"/> and wait for its URL.
        /// Throws <see cref="FormatterException"/> when tctidy cannot start, exits, or prints
        /// anything but a loopback URL within <paramref name="timeout"/>.
        /// </summary>
        public static SettingsServer Start(string exePath, string rootDirectory, TimeSpan timeout)
        {
            if (exePath == null) throw new ArgumentNullException(nameof(exePath));
            if (rootDirectory == null) throw new ArgumentNullException(nameof(rootDirectory));

            var psi = new ProcessStartInfo
            {
                FileName = exePath,
                // No working directory: the server would hold the solution folder open meanwhile.
                Arguments = "--settings " + CommandLine.Quote(rootDirectory),
                UseShellExecute = false,
                CreateNoWindow = true,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                StandardOutputEncoding = Encoding.UTF8,
                StandardErrorEncoding = Encoding.UTF8,
            };

            var process = new Process { StartInfo = psi };
            try
            {
                process.Start();
            }
            catch (Exception e)
            {
                process.Dispose();
                throw new FormatterException(-1, $"could not start tctidy ('{exePath}'): {e.Message}");
            }

            // Keep draining stderr so the server can never block on a full pipe.
            var stderr = new StringBuilder();
            process.ErrorDataReceived += (_, e) =>
            {
                if (e.Data == null) return;
                lock (stderr) stderr.AppendLine(e.Data);
            };
            process.BeginErrorReadLine();

            string? url;
            try
            {
                var firstLine = process.StandardOutput.ReadLineAsync();
                url = firstLine.Wait(timeout) ? firstLine.Result?.Trim() : null;
            }
            catch (AggregateException)
            {
                url = null;
            }
            if (url != null && url.StartsWith(UrlPrefix, StringComparison.Ordinal))
            {
                return new SettingsServer(process, url, rootDirectory);
            }

            Stop(process, waitForOutput: true);
            string message;
            lock (stderr) message = stderr.ToString().Trim();
            process.Dispose();
            const string prefix = "tctidy: ";
            if (message.StartsWith(prefix, StringComparison.Ordinal)) message = message.Substring(prefix.Length);
            throw new FormatterException(
                -1,
                message.Length > 0
                    ? message
                    : url == null
                        ? $"tctidy did not start the settings page within {timeout.TotalSeconds:0}s"
                        : "tctidy did not start the settings page");
        }

        /// <summary>Stop the server (the open page then reports that it has stopped).</summary>
        public void Dispose()
        {
            Stop(_process, waitForOutput: false);
            _process.Dispose();
        }

        private static void Stop(Process process, bool waitForOutput)
        {
            try
            {
                if (!process.HasExited) process.Kill();
                // The parameterless wait also lets the stderr reader reach the end of the pipe,
                // so a failed start reports everything tctidy said.
                if (waitForOutput) process.WaitForExit();
            }
            catch (Exception e) when (e is InvalidOperationException || e is System.ComponentModel.Win32Exception)
            {
                // Already exited.
            }
        }
    }
}
