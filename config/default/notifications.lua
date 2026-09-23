local module = {}

function module.apply_to_config(config)
    config.notifications = {
        -- "toast"  — GPU overlay rendered inside the window
        -- "native" — macOS Notification Center (requires notification permission) (default)
        style = "native",
    }
end

return module
