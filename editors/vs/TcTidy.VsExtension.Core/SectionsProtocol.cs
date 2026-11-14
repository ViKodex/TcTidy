using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.Serialization;
using System.Runtime.Serialization.Json;
using System.Text;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// The JSON exchanged with <c>tctidy --stdin-sections</c>: one request for all the sections, one
    /// response with a result per section, in the same order.
    /// <code>
    /// { "sections": [ { "source": "...", "declaration": false, "filepath": "C:\\Plc\\FB_X.TcPOU" } ] }
    /// { "results": [ { "output": "..." }, { "error": "..." } ] }
    /// </code>
    /// </summary>
    internal static class SectionsProtocol
    {
        public static string Request(IReadOnlyList<SectionRequest> sections)
        {
            var request = new RequestBody { Sections = new List<SectionBody>(sections.Count) };
            foreach (var section in sections)
            {
                request.Sections.Add(new SectionBody
                {
                    Source = section.Source,
                    Declaration = section.Kind == StSection.Declaration,
                    FilePath = string.IsNullOrEmpty(section.SourcePath) ? null : section.SourcePath,
                });
            }

            using var stream = new MemoryStream();
            new DataContractJsonSerializer(typeof(RequestBody)).WriteObject(stream, request);
            return Encoding.UTF8.GetString(stream.ToArray());
        }

        /// <summary>
        /// Reads a response, or null when it is not one: invalid JSON, or a result that is neither
        /// an output nor an error.
        /// </summary>
        public static IReadOnlyList<SectionResult>? Results(string json)
        {
            ResponseBody? response;
            try
            {
                var serializer = new DataContractJsonSerializer(typeof(ResponseBody));
                using var stream = new MemoryStream(Encoding.UTF8.GetBytes(json));
                response = serializer.ReadObject(stream) as ResponseBody;
            }
            catch (Exception ex) when (ex is SerializationException || ex is InvalidCastException || ex is ArgumentException)
            {
                return null;
            }

            if (response?.Results == null) return null;

            var results = new List<SectionResult>(response.Results.Count);
            foreach (var result in response.Results)
            {
                if (result?.Output != null && result.Error == null)
                    results.Add(SectionResult.Formatted(result.Output));
                else if (result?.Error != null && result.Output == null)
                    results.Add(SectionResult.Refused(result.Error));
                else
                    return null;
            }
            return results;
        }

        [DataContract]
        private sealed class RequestBody
        {
            [DataMember(Name = "sections")] public List<SectionBody>? Sections;
        }

        [DataContract]
        private sealed class SectionBody
        {
            [DataMember(Name = "source")] public string? Source;
            [DataMember(Name = "declaration")] public bool Declaration;
            [DataMember(Name = "filepath", EmitDefaultValue = false)] public string? FilePath;
        }

        // Assigned by the serializer only.
#pragma warning disable CS0649
        [DataContract]
        private sealed class ResponseBody
        {
            [DataMember(Name = "results")] public List<ResultBody>? Results;
        }

        [DataContract]
        private sealed class ResultBody
        {
            [DataMember(Name = "output")] public string? Output;
            [DataMember(Name = "error")] public string? Error;
        }
#pragma warning restore CS0649
    }
}
