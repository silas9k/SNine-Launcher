# Phase 8 – Migration und Rollback

Phase 8 verändert das lokale SQLite-Schema nicht. Der Sync-Payload ist mit
Formatkennung `site.s9lab.cloud-payload` und Version 1 unabhängig versioniert.
Der öffentliche Zustand wird bei Bedarf aus den bestehenden Profil-, Inhalts-
und Einstellungsquellen abgeleitet; es gibt keinen neuen veränderlichen
Cloud-Datenbestand, der migriert werden müsste.

Ein Downgrade auf den Phase-7-Checkpoint ignoriert den zusätzlichen IPC-Befehl
und die neue UI. Profile, Inhalte, Accounts und Einstellungen bleiben im
bestehenden Format erhalten. Es existiert kein Remote-Zustand, der bei einem
Rollback divergieren könnte. Eine spätere Provider-Aktivierung benötigt vor
dem ersten Push ein eigenes Migrations-, Idempotenz- und Remote-Rollbackkonzept.
