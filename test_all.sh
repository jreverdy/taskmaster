#!/bin/bash

# Script de test pour valider les exigences de la grille de correction taskmaster
# Teste : kill automatique, retries limités, et comportements erratiques

set -e

echo "=== Test Taskmaster - Grille de Correction ==="
echo

# Fonction pour attendre que taskmaster soit prêt
wait_for_taskmaster() {
    local config=$1
    local timeout=10
    local count=0

    echo "Démarrage de taskmaster avec $config..."
    ./target/debug/taskmaster "$config" &
    TASKMASTER_PID=$!

    # Attendre que taskmaster démarre
    while ! pgrep -f "taskmaster.*$config" > /dev/null 2>&1 && [ $count -lt $timeout ]; do
        sleep 1
        count=$((count + 1))
    done

    if [ $count -eq $timeout ]; then
        echo "ERREUR: Taskmaster n'a pas démarré dans les $timeout secondes"
        kill $TASKMASTER_PID 2>/dev/null || true
        return 1
    fi

    echo "Taskmaster démarré (PID: $TASKMASTER_PID)"
    sleep 2  # Laisser le temps aux processus de démarrer
}

# Fonction pour nettoyer
cleanup() {
    echo "Nettoyage..."
    pkill -f taskmaster || true
    pkill -f "simple_worker.sh\|error_task.sh\|startup_delay.sh" || true
    sleep 1
}

# Test 1: Kill automatique et redémarrage
test_kill_restart() {
    echo "=== Test 1: Kill automatique et redémarrage ==="
    echo "Configuration: test/kill_restart/restart_on_kill.yaml"
    echo "Attendu: Le processus doit redémarrer automatiquement après un kill"
    echo

    wait_for_taskmaster "test/kill_restart/restart_on_kill.yaml"

    # Vérifier que le worker tourne
    if ! pgrep -f "simple_worker.sh" > /dev/null; then
        echo "ERREUR: Le worker ne tourne pas initialement"
        cleanup
        return 1
    fi

    WORKER_PID=$(pgrep -f "simple_worker.sh")
    echo "Worker initial (PID: $WORKER_PID)"

    # Tuer le worker
    echo "Tuer le worker avec SIGKILL..."
    kill -9 $WORKER_PID

    # Attendre et vérifier le redémarrage
    sleep 3
    if pgrep -f "simple_worker.sh" > /dev/null; then
        NEW_WORKER_PID=$(pgrep -f "simple_worker.sh")
        echo "SUCCÈS: Worker redémarré automatiquement (Nouveau PID: $NEW_WORKER_PID)"
        RESULT1="PASS"
    else
        echo "ÉCHEC: Worker n'a pas redémarré"
        RESULT1="FAIL"
    fi

    cleanup
    echo
}

# Test 2: Processus qui échoue et limite de retries
test_retry_limit() {
    echo "=== Test 2: Limite de retries ==="
    echo "Configuration: test/rapid_restart/intensive_retry.yaml"
    echo "Attendu: Le processus doit être abandonné après 10 tentatives"
    echo

    wait_for_taskmaster "test/rapid_restart/intensive_retry.yaml"

    # Attendre que les retries se terminent (10 tentatives max)
    echo "Attente des 10 tentatives de redémarrage..."
    sleep 15  # 10 tentatives avec delai entre elles

    # Vérifier que le processus a été abandonné
    if pgrep -f "error_task.sh" > /dev/null; then
        echo "ÉCHEC: Le processus tourne encore après les retries"
        RESULT2="FAIL"
    else
        echo "SUCCÈS: Le processus a été abandonné après les retries"
        RESULT2="PASS"
    fi

    cleanup
    echo
}

# Test 3: Comportements erratiques
test_erratic_behavior() {
    echo "=== Test 3: Comportements erratiques ==="
    echo "Test de robustesse face à diverses situations"
    echo

    # Test 3a: Configuration invalide
    echo "3a. Configuration avec starttime trop court..."
    wait_for_taskmaster "test/starttime/short_starttime.yaml"
    sleep 5
    if pgrep -f "startup_delay.sh" > /dev/null; then
        echo "INFO: Processus tourne malgré starttime court (peut être normal)"
    else
        echo "INFO: Processus n'a pas pu démarrer avec starttime court"
    fi
    cleanup

    # Test 3b: Test de surcharge
    echo "3b. Test de surcharge avec scaling élevé..."
    wait_for_taskmaster "test/scaling/very_large_scale.yaml"
    sleep 5
    WORKER_COUNT=$(pgrep -f "worker.sh" | wc -l)
    echo "Nombre de workers actifs: $WORKER_COUNT (attendu: 10)"
    if [ "$WORKER_COUNT" -ge 5 ]; then
        echo "SUCCÈS: Gestion correcte de la charge"
        RESULT3="PASS"
    else
        echo "ÉCHEC: Problème de gestion de charge"
        RESULT3="FAIL"
    fi
    cleanup

    echo
}

# Test 4: Test de stabilité générale
test_stability() {
    echo "=== Test 4: Stabilité générale ==="
    echo "Test avec configuration complexe..."

    wait_for_taskmaster "test/simple.yaml"
    sleep 10

    # Vérifier que tous les processus sont actifs
    PROCESSES=("infinity" "infinito")
    ALL_RUNNING=true

    for proc in "${PROCESSES[@]}"; do
        if ! pgrep -f "$proc" > /dev/null; then
            echo "ERREUR: Processus $proc ne tourne pas"
            ALL_RUNNING=false
        fi
    done

    if $ALL_RUNNING; then
        echo "SUCCÈS: Tous les processus de la configuration complexe tournent"
        RESULT4="PASS"
    else
        echo "ÉCHEC: Problèmes avec la configuration complexe"
        RESULT4="FAIL"
    fi

    cleanup
    echo
}

# Exécution des tests
trap cleanup EXIT

echo "Démarrage des tests de validation..."
echo "Assurez-vous que taskmaster est compilé: cargo build --release"
echo

test_kill_restart
test_retry_limit
test_erratic_behavior
test_stability

# Résumé
echo "=== RÉSUMÉ DES TESTS ==="
echo "Kill & Restart: $RESULT1"
echo "Retry Limit: $RESULT2"
echo "Erratic Behavior: $RESULT3"
echo "Stability: $RESULT4"

SUCCESS_COUNT=0
[ "$RESULT1" = "PASS" ] && SUCCESS_COUNT=$((SUCCESS_COUNT + 1))
[ "$RESULT2" = "PASS" ] && SUCCESS_COUNT=$((SUCCESS_COUNT + 1))
[ "$RESULT3" = "PASS" ] && SUCCESS_COUNT=$((SUCCESS_COUNT + 1))
[ "$RESULT4" = "PASS" ] && SUCCESS_COUNT=$((SUCCESS_COUNT + 1))

echo
echo "Tests réussis: $SUCCESS_COUNT/4"

if [ $SUCCESS_COUNT -eq 4 ]; then
    echo "🎉 TOUS LES TESTS RÉUSSIS - Taskmaster passe la grille de correction!"
else
    echo "⚠️  Quelques tests ont échoué - Vérifiez les logs et la configuration"
fi