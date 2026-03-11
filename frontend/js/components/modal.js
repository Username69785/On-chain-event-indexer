import { state, actions } from '../state.js';
import { isAddressLike, validateSolanaAddress, submitAnalyzeRequest, fetchJobInfo } from '../api.js';
import { renderSidebar } from './sidebar.js';
import { renderMainArea } from './mainArea.js';
import { showGlobalToast, showStatus } from '../utils.js';

const POLL_INTERVAL_MS = 5000;

export function initModal() {
    const addAddressBtn = document.getElementById('addAddressBtn');
    const addAddressModal = document.getElementById('addAddressModal');
    const closeModalBtn = document.getElementById('closeModalBtn');
    const form = document.getElementById('indexerForm');
    const submitBtn = document.getElementById('submitBtn');
    const input = document.getElementById('addressInput');
    const clearInputBtn = document.getElementById('clearInputBtn');
    const statusMessage = document.getElementById('statusMessage');

    if (!addAddressModal) return;

    const openModal = () => {
        addAddressModal.classList.remove('hidden');
        input.focus();
    };

    const closeModal = () => {
        addAddressModal.classList.add('hidden');
        statusMessage.classList.add('hidden');
        input.value = '';
        validateInput();
    };

    const validateInput = () => {
        const val = input.value.trim();
        if (val.length > 0) {
            clearInputBtn?.classList.remove('hidden');
        } else {
            clearInputBtn?.classList.add('hidden');
        }
        
        // Disable submit until it looks at least loosely like an address
        if (isAddressLike(val) && val.length > 30) {
            submitBtn.disabled = false;
        } else {
            submitBtn.disabled = true;
        }
    };

    if (clearInputBtn) {
        clearInputBtn.addEventListener('click', () => {
            input.value = '';
            input.focus();
            validateInput();
        });
    }

    input.addEventListener('input', validateInput);

    addAddressBtn.addEventListener('click', openModal);
    closeModalBtn.addEventListener('click', closeModal);
    
    addAddressModal.addEventListener('click', (e) => {
        if (e.target === addAddressModal) closeModal();
    });

    const setupPills = (containerId, filterKey) => {
        const container = document.getElementById(containerId);
        if (!container) return;
        const pills = container.querySelectorAll('.pill');
        pills.forEach(pill => {
            pill.addEventListener('click', () => {
                pills.forEach(p => p.classList.remove('active'));
                pill.classList.add('active');
                actions.updateFilter(filterKey, parseInt(pill.dataset.value, 10));
            });
        });
    };

    setupPills('timeFilters', 'time');
    setupPills('txFilters', 'txLimit');

    const setLoadingUI = (loading) => {
        actions.setLoading(loading);
        const btnText = submitBtn.querySelector('span');
        if (loading) {
            submitBtn.disabled = true;
            btnText.textContent = 'Processing...';
            submitBtn.classList.add('loading');
        } else {
            submitBtn.disabled = false;
            btnText.textContent = 'Analyze';
            submitBtn.classList.remove('loading');
        }
    };

    const stopPolling = (address) => {
        actions.clearPollingInterval(address);
    };

    const syncJobInfo = async (address, jobId) => {
        const currentItem = state.addresses.find(item => item.address === address);
        if (!currentItem) {
            stopPolling(address);
            return;
        }

        try {
            const jobInfo = await fetchJobInfo(jobId);
            const previousStatus = currentItem.status;

            actions.addOrUpdateAddress(address, {
                jobId,
                status: jobInfo.status,
                totalTransactions: jobInfo.total_transactions ?? 0,
                processedTransactions: jobInfo.processed_transactions ?? 0,
                remainingTransactions: jobInfo.remaining_transactions ?? 0,
                updatedAt: jobInfo.updated_at ?? null
            });

            renderSidebar();
            renderMainArea();

            if (previousStatus !== jobInfo.status) {
                showGlobalToast(`Status changed: ${jobInfo.status}`);
            }

            if (jobInfo.status === 'ready' || jobInfo.status === 'error') {
                stopPolling(address);
            }
        } catch (error) {
            console.error('Polling error:', error);
        }
    };

    const startPolling = (address, jobId) => {
        stopPolling(address);
        const intervalId = window.setInterval(() => {
            void syncJobInfo(address, jobId);
        }, POLL_INTERVAL_MS);

        actions.setPollingInterval(address, intervalId);
        void syncJobInfo(address, jobId);
    };

    const handleAnalyze = async (e) => {
        e?.preventDefault();
        e?.stopPropagation();

        if (state.isLoading) return;

        const address = input.value.trim();

        if (!address) {
            showStatus('Please enter a valid address', 'error');
            return;
        }

        if (!isAddressLike(address)) {
            showStatus('Invalid input data: value is not an address', 'error');
            return;
        }

        const validation = validateSolanaAddress(address);
        if (!validation.isValid) {
            showStatus(validation.message, 'error');
            return;
        }

        setLoadingUI(true);

        try {
            actions.addOrUpdateAddress(address, {
                status: 'pending',
                totalTransactions: 0,
                processedTransactions: 0,
                remainingTransactions: 0
            });
            renderSidebar();
            renderMainArea();

            const result = await submitAnalyzeRequest(address, state.filters);
            console.log('Server response:', result);

            if (result.job_id) {
                actions.addOrUpdateAddress(address, { jobId: result.job_id });
                startPolling(address, result.job_id);
            }

            showStatus(`Successfully started indexing for: ${address.slice(0, 4)}...${address.slice(-4)}`, 'success');
            setTimeout(() => closeModal(), 1000);
        } catch (error) {
            console.error('Error:', error);
            actions.addOrUpdateAddress(address, 'error');
            renderSidebar();
            renderMainArea();
            showStatus(error.message || 'An error occurred. Please try again.', 'error');
        } finally {
            setLoadingUI(false);
        }
    };

    form.addEventListener('submit', handleAnalyze);
    submitBtn.addEventListener('click', handleAnalyze);
}
