using System.Text.Json.Serialization;

namespace WecomAgent;

public sealed class AgentRequest
{
    [JsonPropertyName("id")]
    public string Id { get; set; } = "";

    [JsonPropertyName("cmd")]
    public string Cmd { get; set; } = "";

    [JsonPropertyName("launchWecom")]
    public bool LaunchWecom { get; set; }

    [JsonPropertyName("timeoutSec")]
    public int TimeoutSec { get; set; } = 30;

    [JsonPropertyName("retryCount")]
    public int RetryCount { get; set; }

    [JsonPropertyName("contact")]
    public string? Contact { get; set; }

    [JsonPropertyName("message")]
    public string? Message { get; set; }

    [JsonPropertyName("closeAfterSend")]
    public bool CloseAfterSend { get; set; } = true;
}

public sealed class AgentStep
{
    [JsonPropertyName("title")]
    public string Title { get; set; } = "";

    [JsonPropertyName("status")]
    public string Status { get; set; } = "ok";

    [JsonPropertyName("time")]
    public string Time { get; set; } = "";

    [JsonPropertyName("note")]
    public string Note { get; set; } = "";
}

public sealed class AgentResponse
{
    [JsonPropertyName("id")]
    public string Id { get; set; } = "";

    [JsonPropertyName("ok")]
    public bool Ok { get; set; }

    [JsonPropertyName("note")]
    public string Note { get; set; } = "";

    [JsonPropertyName("steps")]
    public List<AgentStep> Steps { get; set; } = new();
}
