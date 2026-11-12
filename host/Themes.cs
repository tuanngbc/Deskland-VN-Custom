using System;
using System.Collections.Generic;
using System.IO;
using System.Text;

// 主题清单（prototype/themes.json）：页面映射与产品权限共用同一份清单。
// 未标 product 的 ready 主题默认拒绝，避免新增付费主题被意外免费开放。
static class Themes
{
    static string cachedPath;
    static DateTime cachedStamp;
    static Dictionary<string, object> cachedRoot;
    static Dictionary<string, object> Find(string pageDir, string id)
    {
        if (id == null) return null;
        try
        {
            string path = Path.Combine(pageDir, "themes.json");
            DateTime stamp = File.GetLastWriteTimeUtc(path);
            if (cachedRoot == null || cachedPath != path || stamp != cachedStamp)
            {
                cachedRoot = Json.Parse(File.ReadAllText(path, Encoding.UTF8)) as Dictionary<string, object>;
                cachedPath = path; cachedStamp = stamp;
            }
            var root = cachedRoot;
            if (root == null) return null;
            foreach (object o in (System.Collections.IEnumerable)root["themes"])
            {
                var theme = o as Dictionary<string, object>;
                if (theme != null && (theme["id"] as string) == id &&
                    theme.ContainsKey("status") && (theme["status"] as string) == "ready")
                    return theme;
            }
        }
        catch (Exception ex) { Log.Write("读取 themes.json 失败：" + ex.Message); }
        return null;
    }

    public static string PageFor(string pageDir, string id)
    {
        var theme = Find(pageDir, id);
        string page = theme == null || !theme.ContainsKey("page") ? null : theme["page"] as string;
        if (string.IsNullOrEmpty(page) || Path.IsPathRooted(page) || page.Contains("..") || !page.EndsWith(".html", StringComparison.OrdinalIgnoreCase)) return null;
        return File.Exists(Path.Combine(pageDir, page)) ? page : null;
    }

    public static bool ValidParameter(string pageDir, string theme, string key, object value)
    {
        if (value == null || value is bool || value is string) return false;
        double number;
        try { number = Convert.ToDouble(value); } catch { return false; }
        if (double.IsNaN(number) || double.IsInfinity(number)) return false;
        if (key == "seed") return number >= 0 && number <= int.MaxValue && number == Math.Floor(number);
        var t = Find(pageDir, theme);
        if (t == null) return false;
        if (key == "adaptive") return t.ContainsKey("adaptive") && (number == 0 || number == 1);
        object groups;
        if (!t.TryGetValue("groups", out groups)) return false;
        foreach (object group in (System.Collections.IEnumerable)groups)
        foreach (object parameter in (System.Collections.IEnumerable)((Dictionary<string, object>)group)["params"])
        {
            var p = (Dictionary<string, object>)parameter;
            if ((p["key"] as string) != key) continue;
            if (p.ContainsKey("options"))
            {
                var options = (object[])p["options"];
                return number >= 0 && number < options.Length && number == Math.Floor(number);
            }
            return number >= Convert.ToDouble(p["min"]) && number <= Convert.ToDouble(p["max"]);
        }
        return false;
    }

    public static string ProductFor(string pageDir, string id)
    {
        var theme = Find(pageDir, id);
        object product;
        return theme != null && theme.TryGetValue("product", out product) ? product as string : null;
    }

    public static bool Allowed(string pageDir, string id, Func<string, bool> owns)
    {
        if (PageFor(pageDir, id) == null) return false;
        string product = ProductFor(pageDir, id);
        if (product == "free") return true;
        return product != null && owns != null && owns(product);
    }
}
