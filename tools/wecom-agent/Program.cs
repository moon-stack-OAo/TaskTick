using System.Text;
using System.Text.Json;
using WecomAgent;

// one-shot：读 stdin 第一行 JSON 请求，写一行 JSON 响应后退出
Console.InputEncoding = Encoding.UTF8;
Console.OutputEncoding = Encoding.UTF8;

var jsonOpts = new JsonSerializerOptions
{
    PropertyNameCaseInsensitive = true,
    DefaultIgnoreCondition = System.Text.Json.Serialization.JsonIgnoreCondition.WhenWritingNull,
};

string? line;
try
{
    line = Console.In.ReadLine();
}
catch (Exception ex)
{
    Write(Fail("0", $"读取 stdin 失败: {ex.Message}"));
    return 2;
}

if (string.IsNullOrWhiteSpace(line))
{
    Write(Fail("0", "空请求"));
    return 2;
}

AgentRequest? req;
try
{
    req = JsonSerializer.Deserialize<AgentRequest>(line, jsonOpts);
}
catch (Exception ex)
{
    Write(Fail("0", $"JSON 解析失败: {ex.Message}"));
    return 2;
}

if (req == null || string.IsNullOrWhiteSpace(req.Cmd))
{
    Write(Fail(req?.Id ?? "0", "缺少 cmd"));
    return 2;
}

if (string.IsNullOrWhiteSpace(req.Id))
    req.Id = Guid.NewGuid().ToString("N")[..8];

AgentResponse resp = req.Cmd.Trim().ToLowerInvariant() switch
{
    "ping" => new AgentResponse
    {
        Id = req.Id,
        Ok = true,
        Note = "wecom-agent pong · FlaUI.UIA3",
        Steps =
        [
            new AgentStep
            {
                Title = "ping",
                Status = "ok",
                Time = DateTime.Now.ToString("HH:mm:ss"),
                Note = "alive",
            },
        ],
    },
    "probe" => WecomAutomation.Probe(req),
    "send" => WecomAutomation.Send(req),
    _ => Fail(req.Id, $"未知 cmd: {req.Cmd}"),
};

Write(resp);
return resp.Ok ? 0 : 1;

static void Write(AgentResponse resp)
{
    var json = JsonSerializer.Serialize(resp);
    Console.Out.WriteLine(json);
    Console.Out.Flush();
}

static AgentResponse Fail(string id, string note) => new()
{
    Id = id,
    Ok = false,
    Note = note,
    Steps =
    [
        new AgentStep
        {
            Title = "错误",
            Status = "fail",
            Time = DateTime.Now.ToString("HH:mm:ss"),
            Note = note,
        },
    ],
};
