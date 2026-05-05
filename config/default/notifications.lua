local module = {}

function module.apply_to_config(config)
    config.notifications = {
        -- "toast"  — GPU overlay rendered inside the window (default)
        -- "native" — macOS Notification Center (requires notification permission)
        style = "toast",
    }
end

return module
