/* Private Weston test module: exercises GDK's actual xdg_foreign callbacks.
 * It supplies test handles to the fake portal, not real dialog parenting.
 * Never built into or shipped with Prollyglot. */
#include <libweston/libweston.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "xdg-foreign-server.h"

static FILE *events;
static unsigned sequence;
static bool stall;

static void record(const char *event)
{
    fprintf(events, "%s\n", event);
    fflush(events);
}

static void destroy(struct wl_client *client, struct wl_resource *resource)
{
    (void)client;
    wl_resource_destroy(resource);
}

static void released(struct wl_resource *resource)
{
    (void)resource;
    record("release");
}

static const struct zxdg_exported_v2_interface exported_impl = { .destroy = destroy };

static void export_toplevel(struct wl_client *client, struct wl_resource *resource,
                           uint32_t id, struct wl_resource *surface)
{
    (void)resource;
    (void)surface;
    struct wl_resource *exported = wl_resource_create(client, &zxdg_exported_v2_interface, 1, id);
    if (!exported) {
        wl_client_post_no_memory(client);
        return;
    }
    wl_resource_set_implementation(exported, &exported_impl, NULL, released);
    record("export");
    if (!stall) {
        char handle[64];
        snprintf(handle, sizeof(handle), "prollyglot-fixture-%u", ++sequence);
        zxdg_exported_v2_send_handle(exported, handle);
    }
}

static const struct zxdg_exporter_v2_interface exporter_impl = {
    .destroy = destroy, .export_toplevel = export_toplevel
};

static void bind_exporter(struct wl_client *client, void *data, uint32_t version, uint32_t id)
{
    (void)data;
    (void)version;
    struct wl_resource *resource = wl_resource_create(client, &zxdg_exporter_v2_interface, 1, id);
    if (!resource) {
        wl_client_post_no_memory(client);
        return;
    }
    wl_resource_set_implementation(resource, &exporter_impl, NULL, NULL);
}

WL_EXPORT int wet_module_init(struct weston_compositor *compositor, int *argc, char *argv[])
{
    (void)argc;
    (void)argv;
    const char *root = getenv("PROLLYGLOT_PRIVATE_PIPEWIRE");
    const char *runtime = getenv("XDG_RUNTIME_DIR");
    const char *socket = getenv("WAYLAND_DISPLAY");
    const char *state = getenv("PROLLYGLOT_DESKTOP_FIXTURE_STATE");
    const char *mode = getenv("PROLLYGLOT_WAYLAND_EXPORT_MODE");
    if (!root || !runtime || strcmp(root, runtime) ||
        !strstr(root, "/prollyglot-pipewire-") || getenv("DISPLAY") ||
        !socket || strncmp(socket, "prollyglot-", 10) ||
        !state || strncmp(state, root, strlen(root)) || state[strlen(root)] != '/' ||
        !mode || (strcmp(mode, "normal") && strcmp(mode, "stall")))
        return -1;
    char path[4096];
    if (snprintf(path, sizeof(path), "%s/exports.log", state) >= (int)sizeof(path))
        return -1;
    events = fopen(path, "w");
    if (!events)
        return -1;
    stall = !strcmp(mode, "stall");
    return wl_global_create(compositor->wl_display, &zxdg_exporter_v2_interface,
                            1, NULL, bind_exporter) ? 0 : -1;
}
