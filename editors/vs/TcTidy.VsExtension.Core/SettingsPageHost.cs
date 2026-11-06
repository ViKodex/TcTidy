using System;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// Keeps at most one settings server alive for the IDE and hands out its page: asking again for
    /// the same solution reopens the running server's page instead of starting a second server;
    /// another solution, or a server that stopped on its own, gets a fresh one. Thread-safe.
    /// </summary>
    public sealed class SettingsPageHost : IDisposable
    {
        private readonly Func<string, string, ISettingsServer> _start;
        private readonly object _gate = new object();
        private ISettingsServer? _current;

        /// <param name="start">Starts a server from <c>(exePath, rootDirectory)</c>.</param>
        public SettingsPageHost(Func<string, string, ISettingsServer> start)
        {
            _start = start ?? throw new ArgumentNullException(nameof(start));
        }

        /// <summary>The settings page that edits the configuration of <paramref name="rootDirectory"/>.</summary>
        public string Open(string exePath, string rootDirectory)
        {
            lock (_gate)
            {
                if (_current != null && _current.IsRunning && SolutionDirectory.Same(_current.RootDirectory, rootDirectory))
                {
                    return _current.Url;
                }
                StopCurrent();
                _current = _start(exePath, rootDirectory);
                return _current.Url;
            }
        }

        /// <summary>Stop the running server, if any (e.g. when its solution closes).</summary>
        public void Stop()
        {
            lock (_gate) StopCurrent();
        }

        public void Dispose() => Stop();

        private void StopCurrent()
        {
            var current = _current;
            _current = null;
            current?.Dispose();
        }
    }
}
