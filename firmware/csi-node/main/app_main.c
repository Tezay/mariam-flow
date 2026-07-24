/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * csi-node firmware: TX blasts reference ESP-NOW traffic on a fixed
 * channel; RX extracts CSI from that traffic and streams one CSI_DATA
 * text line per UDP datagram to the edge (see docs/adr/0005 and 0007 of
 * the repository).
 *
 * Based on the `csi_send` / `csi_recv` get-started examples of
 * espressif/esp-csi (Apache License 2.0); original notices preserved.
 *
 * STATUS: written against ESP-IDF 5.x APIs and the esp-csi examples,
 * NOT yet compiled or flashed — to be validated on the first hardware.
 */

#include <string.h>

#include "freertos/FreeRTOS.h"
#include "freertos/event_groups.h"
#include "freertos/queue.h"
#include "freertos/task.h"

#include "esp_event.h"
#include "esp_log.h"
#include "esp_mac.h"
#include "esp_netif.h"
#include "esp_now.h"
#include "esp_timer.h"
#include "esp_wifi.h"
#include "nvs_flash.h"

#include "lwip/sockets.h"

static const char *TAG = "csi-node";

/*
 * The transmitter overrides its MAC with this fixed address (same value
 * as the esp-csi examples): the RX filter and the edge's --tx-mac option
 * know the reference transmitter a priori, on every deployment.
 */
static const uint8_t TX_MAC[6] = {0x1a, 0x00, 0x00, 0x00, 0x00, 0x00};

/* ------------------------------------------------------------------ */
/* Shared Wi-Fi bring-up                                               */
/* ------------------------------------------------------------------ */

static void wifi_base_init(void)
{
    esp_err_t err = nvs_flash_init();
    if (err == ESP_ERR_NVS_NO_FREE_PAGES || err == ESP_ERR_NVS_NEW_VERSION_FOUND) {
        ESP_ERROR_CHECK(nvs_flash_erase());
        ESP_ERROR_CHECK(nvs_flash_init());
    }
    ESP_ERROR_CHECK(esp_netif_init());
    ESP_ERROR_CHECK(esp_event_loop_create_default());

    wifi_init_config_t cfg = WIFI_INIT_CONFIG_DEFAULT();
    ESP_ERROR_CHECK(esp_wifi_init(&cfg));
    ESP_ERROR_CHECK(esp_wifi_set_storage(WIFI_STORAGE_RAM));
}

/* ------------------------------------------------------------------ */
/* TX role: reference traffic generator                                */
/* ------------------------------------------------------------------ */

#if CONFIG_CSI_NODE_ROLE_TX

static void run_tx(void)
{
    wifi_base_init();
    ESP_ERROR_CHECK(esp_wifi_set_mode(WIFI_MODE_STA));
    ESP_ERROR_CHECK(esp_wifi_start());
    ESP_ERROR_CHECK(esp_wifi_set_ps(WIFI_PS_NONE));
    ESP_ERROR_CHECK(
        esp_wifi_set_channel(CONFIG_CSI_NODE_TX_CHANNEL, WIFI_SECOND_CHAN_NONE));
    ESP_ERROR_CHECK(esp_wifi_set_mac(WIFI_IF_STA, TX_MAC));

    ESP_ERROR_CHECK(esp_now_init());
    esp_now_peer_info_t peer = {
        .channel = CONFIG_CSI_NODE_TX_CHANNEL,
        .ifidx = WIFI_IF_STA,
        .encrypt = false,
        .peer_addr = {0xff, 0xff, 0xff, 0xff, 0xff, 0xff},
    };
    ESP_ERROR_CHECK(esp_now_add_peer(&peer));

    /* Force a fixed HT20 rate. The default ESP-NOW rate is legacy 11b,
     * which carries no HT training fields, so the receiver's CSI engine
     * (acquire_csi_ht20) would see nothing. MCS0 is the most robust HT
     * rate. Mirrors the esp-csi csi_send example. */
    esp_now_rate_config_t rate = {
        .phymode = WIFI_PHY_MODE_HT20,
        .rate = WIFI_PHY_RATE_MCS0_LGI,
        .ersu = false,
        .dcm = false,
    };
    ESP_ERROR_CHECK(esp_now_set_peer_rate_config(peer.peer_addr, &rate));

    ESP_LOGI(TAG, "tx: channel %d, %d frames/s, HT20 MCS0", CONFIG_CSI_NODE_TX_CHANNEL,
             CONFIG_CSI_NODE_SEND_RATE_HZ);

    const TickType_t period = pdMS_TO_TICKS(1000 / CONFIG_CSI_NODE_SEND_RATE_HZ);
    uint32_t count = 0;
    for (;;) {
        esp_err_t send_err =
            esp_now_send(peer.peer_addr, (const uint8_t *)&count, sizeof(count));
        if (send_err != ESP_OK) {
            ESP_LOGW(TAG, "esp_now_send: %s", esp_err_to_name(send_err));
        }
        count += 1;
        vTaskDelay(period > 0 ? period : 1);
    }
}

#endif /* CONFIG_CSI_NODE_ROLE_TX */

/* ------------------------------------------------------------------ */
/* RX role: CSI extraction and streaming                               */
/* ------------------------------------------------------------------ */

#if CONFIG_CSI_NODE_ROLE_RX

/* One formatted CSI_DATA line. Sized for the C6 layout with margin. */
#define CSI_LINE_MAX 3072
#define CSI_QUEUE_DEPTH 8

typedef struct {
    uint16_t len;
    char text[CSI_LINE_MAX];
} csi_line_t;

static uint32_t s_seq;

#if CONFIG_CSI_NODE_RX_MODE_JOIN
static QueueHandle_t s_line_queue;
static EventGroupHandle_t s_wifi_events;
static uint32_t s_dropped_lines;

#define WIFI_CONNECTED_BIT BIT0

static void on_wifi_event(void *arg, esp_event_base_t base, int32_t id, void *data)
{
    if (base == WIFI_EVENT && id == WIFI_EVENT_STA_START) {
        esp_wifi_connect();
    } else if (base == WIFI_EVENT && id == WIFI_EVENT_STA_DISCONNECTED) {
        ESP_LOGW(TAG, "disconnected, retrying");
        esp_wifi_connect();
    } else if (base == IP_EVENT && id == IP_EVENT_STA_GOT_IP) {
        xEventGroupSetBits(s_wifi_events, WIFI_CONNECTED_BIT);
    }
}
#endif /* CONFIG_CSI_NODE_RX_MODE_JOIN */

/*
 * CSI callback: runs in the Wi-Fi task — keep it minimal. Format the
 * line and enqueue it; networking happens in the sender task. A full
 * queue drops the line (counted): losing a frame is fine, blocking the
 * Wi-Fi task is not.
 */
static void on_csi(void *ctx, wifi_csi_info_t *info)
{
    if (!info || !info->buf || info->len == 0) {
        return;
    }
    if (memcmp(info->mac, TX_MAC, sizeof(TX_MAC)) != 0) {
        return;
    }

    static csi_line_t line; /* Wi-Fi task only: no concurrent access. */
    const wifi_pkt_rx_ctrl_t *rx = &info->rx_ctrl;

    s_seq += 1;
    /* The C6 rx_ctrl (esp_wifi_rxctrl_t) exposes no fft_gain / agc_gain
     * fields on ESP-IDF 5.5.x; those two columns are diagnostic values the
     * edge parser accepts but does not use, so we emit 0 to preserve the
     * 15-column CSI_DATA layout the parser detects (ADR 0005). */
    int written = snprintf(
        line.text, sizeof(line.text),
        "CSI_DATA,%u," MACSTR ",%d,%d,%d,%d,%d,%d,%u,%u,%d,%d,%d,\"[",
        (unsigned)s_seq, MAC2STR(info->mac), rx->rssi, rx->rate, rx->noise_floor,
        0, 0, rx->channel, (unsigned)rx->timestamp,
        (unsigned)rx->sig_len, rx->cur_bb_format, info->len,
        info->first_word_invalid ? 1 : 0);
    if (written < 0 || written >= (int)sizeof(line.text)) {
        return;
    }

    for (uint16_t i = 0; i < info->len; i++) {
        int n = snprintf(line.text + written, sizeof(line.text) - (size_t)written,
                         i == 0 ? "%d" : ",%d", info->buf[i]);
        if (n < 0 || written + n >= (int)sizeof(line.text) - 3) {
            return; /* oversized frame: drop rather than truncate */
        }
        written += n;
    }
    written += snprintf(line.text + written, sizeof(line.text) - (size_t)written, "]\"");
    line.len = (uint16_t)written;

#if CONFIG_CSI_NODE_RX_MODE_FIXED
    /* Bring-up mode: the line goes straight to the serial console. */
    printf("%.*s\n", (int)line.len, line.text);
#else /* CONFIG_CSI_NODE_RX_MODE_JOIN */
#if CONFIG_CSI_NODE_SERIAL_OUTPUT
    printf("%.*s\n", (int)line.len, line.text);
#endif
    if (xQueueSend(s_line_queue, &line, 0) != pdTRUE) {
        s_dropped_lines += 1;
        if ((s_dropped_lines % 100) == 1) {
            ESP_LOGW(TAG, "line queue full, dropped %u", (unsigned)s_dropped_lines);
        }
    }
#endif
}

#if CONFIG_CSI_NODE_RX_MODE_JOIN
static void udp_sender_task(void *arg)
{
    struct sockaddr_in edge = {
        .sin_family = AF_INET,
        .sin_port = htons(CONFIG_CSI_NODE_EDGE_PORT),
    };
    edge.sin_addr.s_addr = inet_addr(CONFIG_CSI_NODE_EDGE_HOST);

    int sock = socket(AF_INET, SOCK_DGRAM, IPPROTO_IP);
    if (sock < 0) {
        ESP_LOGE(TAG, "socket() failed, UDP output disabled");
        vTaskDelete(NULL);
        return;
    }
    ESP_LOGI(TAG, "streaming to %s:%d", CONFIG_CSI_NODE_EDGE_HOST,
             CONFIG_CSI_NODE_EDGE_PORT);

    static csi_line_t line;
    for (;;) {
        if (xQueueReceive(s_line_queue, &line, portMAX_DELAY) == pdTRUE) {
            /* One line per datagram (ADR 0007); losses are acceptable
             * and measured end-to-end via the sequence numbers. */
            sendto(sock, line.text, line.len, 0, (struct sockaddr *)&edge,
                   sizeof(edge));
        }
    }
}
#endif /* CONFIG_CSI_NODE_RX_MODE_JOIN */

/* Enables CSI acquisition and registers the callback. Shared by both RX
 * modes; the same acquisition switches as the esp-csi C6 example. */
static void start_csi(void)
{
    wifi_csi_config_t csi = {
        .enable = true,
        .acquire_csi_legacy = false,
        .acquire_csi_ht20 = true,
        .acquire_csi_ht40 = true,
        .acquire_csi_su = true,
        .acquire_csi_mu = true,
        .acquire_csi_dcm = true,
        .acquire_csi_beamformed = true,
    };
    ESP_ERROR_CHECK(esp_wifi_set_csi_config(&csi));
    ESP_ERROR_CHECK(esp_wifi_set_csi_rx_cb(&on_csi, NULL));
    ESP_ERROR_CHECK(esp_wifi_set_csi(true));
}

#if CONFIG_CSI_NODE_RX_MODE_FIXED

/*
 * Bring-up mode: park on a fixed channel and listen, with no scan and no
 * association (mirrors the esp-csi csi_recv example). Promiscuous mode
 * delivers the TX's ESP-NOW broadcasts to the CSI engine; lines go to the
 * serial console. Needs no network at all.
 */
static void run_rx(void)
{
    wifi_base_init();
    ESP_ERROR_CHECK(esp_wifi_set_mode(WIFI_MODE_STA));
    ESP_ERROR_CHECK(esp_wifi_start());
    /* Power save must be off: it decimates the CSI capture rate. */
    ESP_ERROR_CHECK(esp_wifi_set_ps(WIFI_PS_NONE));
    ESP_ERROR_CHECK(
        esp_wifi_set_channel(CONFIG_CSI_NODE_RX_CHANNEL, WIFI_SECOND_CHAN_NONE));

    ESP_ERROR_CHECK(esp_now_init());
    esp_now_peer_info_t peer = {
        .channel = CONFIG_CSI_NODE_RX_CHANNEL,
        .ifidx = WIFI_IF_STA,
        .encrypt = false,
        .peer_addr = {0xff, 0xff, 0xff, 0xff, 0xff, 0xff},
    };
    ESP_ERROR_CHECK(esp_now_add_peer(&peer));
    ESP_ERROR_CHECK(esp_wifi_set_promiscuous(true));

    start_csi();
    ESP_LOGI(TAG,
             "rx: fixed channel %d, serial output — the TX must use the same channel",
             CONFIG_CSI_NODE_RX_CHANNEL);
}

#else /* CONFIG_CSI_NODE_RX_MODE_JOIN */

/*
 * Production mode: join the edge access point (STA), then stream CSI over
 * UDP. The capture channel is the AP's channel, which the TX must match.
 */
static void run_rx(void)
{
    s_wifi_events = xEventGroupCreate();
    wifi_base_init();

    esp_netif_create_default_wifi_sta();
    ESP_ERROR_CHECK(esp_event_handler_register(WIFI_EVENT, ESP_EVENT_ANY_ID,
                                               &on_wifi_event, NULL));
    ESP_ERROR_CHECK(esp_event_handler_register(IP_EVENT, IP_EVENT_STA_GOT_IP,
                                               &on_wifi_event, NULL));

    wifi_config_t sta = {0};
    strlcpy((char *)sta.sta.ssid, CONFIG_CSI_NODE_WIFI_SSID, sizeof(sta.sta.ssid));
    strlcpy((char *)sta.sta.password, CONFIG_CSI_NODE_WIFI_PASSWORD,
            sizeof(sta.sta.password));
    ESP_ERROR_CHECK(esp_wifi_set_mode(WIFI_MODE_STA));
    ESP_ERROR_CHECK(esp_wifi_set_config(WIFI_IF_STA, &sta));
    ESP_ERROR_CHECK(esp_wifi_start());
    /* Power save must be off: it decimates the CSI capture rate. */
    ESP_ERROR_CHECK(esp_wifi_set_ps(WIFI_PS_NONE));

    xEventGroupWaitBits(s_wifi_events, WIFI_CONNECTED_BIT, pdFALSE, pdTRUE,
                        portMAX_DELAY);

    uint8_t channel = 0;
    wifi_second_chan_t second = WIFI_SECOND_CHAN_NONE;
    ESP_ERROR_CHECK(esp_wifi_get_channel(&channel, &second));
    ESP_LOGI(TAG, "rx: associated on channel %u — the TX node must use it",
             channel);

    s_line_queue = xQueueCreate(CSI_QUEUE_DEPTH, sizeof(csi_line_t));
    xTaskCreate(udp_sender_task, "csi_udp", 4096, NULL, 5, NULL);

    start_csi();
    ESP_LOGI(TAG, "rx: CSI streaming started");
}

#endif /* CONFIG_CSI_NODE_RX_MODE_FIXED */

#endif /* CONFIG_CSI_NODE_ROLE_RX */

void app_main(void)
{
#if CONFIG_CSI_NODE_ROLE_TX
    run_tx();
#else
    run_rx();
#endif
}
