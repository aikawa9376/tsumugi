/*
 * GTK/X11 embedding feasibility trial, separate from the application console.
 * Build with:
 *   cc kitty_probe.c -o /tmp/kitty_probe $(pkg-config --cflags --libs gtk+-3.0) -lX11
 * Run with GDK_BACKEND=x11. It hosts a private kitty process using the user's
 * configuration, with X11 and its control socket overridden for this trial.
 * The shell here is a temporary shell, not a Tsumugi-managed session.
 * Closing the window stops that process; the trial also expires after 10 minutes.
 */
#include <gtk/gtk.h>
#include <gtk/gtkx.h>
#include <gdk/gdkx.h>
#include <X11/Xlib.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>
#include <stdlib.h>

static GtkWidget *socket_widget;
static GPid kitty_pid;
static int attempts;
static char child_title[100];
static char control_address[160];

static void cleanup(void) {
    if (kitty_pid > 0) {
        kill(kitty_pid, SIGTERM);
        waitpid(kitty_pid, NULL, 0);
        g_spawn_close_pid(kitty_pid);
        kitty_pid = 0;
    }
}

static void finish(void) { cleanup(); gtk_main_quit(); }

static gboolean timeout_finish(gpointer unused) {
    (void)unused;
    finish();
    return G_SOURCE_REMOVE;
}

static gboolean find_kitty(gpointer unused) {
    (void)unused;
    Display *display = GDK_DISPLAY_XDISPLAY(gdk_display_get_default());
    gdk_x11_display_error_trap_push(gdk_display_get_default());
    Window root, parent, *children = NULL;
    unsigned int count = 0;
    XQueryTree(display, DefaultRootWindow(display), &root, &parent, &children, &count);
    Window found = 0;
    for (unsigned int i = 0; i < count; i++) {
        char *name = NULL;
        if (XFetchName(display, children[i], &name) && name) {
            if (strcmp(name, child_title) == 0) found = children[i];
            XFree(name);
        }
    }
    if (children) XFree(children);
    XSync(display, False);
    gdk_x11_display_error_trap_pop_ignored(gdk_display_get_default());
    if (!found) {
        if (++attempts > 100) {
            fprintf(stderr, "KITTY_EMBED_ERROR no child window\n");
            finish();
            return G_SOURCE_REMOVE;
        }
        return G_SOURCE_CONTINUE;
    }
    gtk_socket_add_id(GTK_SOCKET(socket_widget), found);
    XMapWindow(display, found);
    XSetInputFocus(display, found, RevertToParent, CurrentTime);
    XSync(display, False);
    gtk_widget_grab_focus(socket_widget);
    printf("KITTY_EMBED_ATTACHED xid=%lu pid=%d control=%s\n", found, kitty_pid, control_address);
    fflush(stdout);
    return G_SOURCE_REMOVE;
}

int main(int argc, char **argv) {
    gtk_init(&argc, &argv);
    atexit(cleanup);
    snprintf(child_title, sizeof child_title, "Tsumugi embedded kitty child %d", getpid());
    snprintf(control_address, sizeof control_address, "unix:/tmp/tsumugi-kitty-embed-probe-%d.sock", getpid());
    if (!GDK_IS_X11_DISPLAY(gdk_display_get_default())) return 3;
    GtkWidget *window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
    gtk_window_set_title(GTK_WINDOW(window), "Tsumugi kitty embed probe");
    gtk_window_set_default_size(GTK_WINDOW(window), 900, 550);
    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_container_add(GTK_CONTAINER(window), box);
    GtkWidget *label = gtk_label_new("kitty embedded below — GTK/X11 feasibility probe");
    gtk_box_pack_start(GTK_BOX(box), label, FALSE, FALSE, 12);
    socket_widget = gtk_socket_new();
    gtk_box_pack_start(GTK_BOX(box), socket_widget, TRUE, TRUE, 0);
    g_signal_connect(window, "destroy", G_CALLBACK(finish), NULL);
    gtk_widget_show_all(window);
    gchar *child[] = {"kitty", "--class", "tsumugi-kitty-embed-probe", "--title", child_title, "--start-as=hidden", "-o", "linux_display_server=x11", "-o", "hide_window_decorations=yes", "--listen-on", control_address, "-o", "allow_remote_control=socket-only", "/bin/bash", "--noprofile", "--norc", NULL};
    GError *error = NULL;
    if (!g_spawn_async(NULL, child, NULL, G_SPAWN_SEARCH_PATH | G_SPAWN_DO_NOT_REAP_CHILD, NULL, NULL, &kitty_pid, &error)) {
        fprintf(stderr, "KITTY_EMBED_ERROR %s\n", error->message);
        return 1;
    }
    g_timeout_add(100, find_kitty, NULL);
    g_timeout_add_seconds(600, timeout_finish, NULL);
    gtk_main();
    return 0;
}
