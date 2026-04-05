export function showStatus(message, type) {
    const statusMessage = document.getElementById('statusMessage');
    statusMessage.textContent = message;
    statusMessage.className = `status-message ${type}`;
    statusMessage.classList.remove('hidden');

    if (type === 'success') {
        setTimeout(() => {
            if (!statusMessage.classList.contains('hidden')) {
                statusMessage.classList.add('hidden');
            }
        }, 5000);
    }
}

export function showGlobalToast(message) {
    const toast = document.getElementById('globalToast');
    if (!toast) return;
    toast.textContent = message;
    toast.classList.add('show');
    
    if (toast.timeoutId) clearTimeout(toast.timeoutId);
    
    toast.timeoutId = setTimeout(() => {
        toast.classList.remove('show');
    }, 2500);
}
