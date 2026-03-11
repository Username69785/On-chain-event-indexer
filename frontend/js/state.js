export const state = {
    isLoading: false,
    addresses: [],
    activeAddress: null,
    pollingIntervals: {},
    filters: {
        time: 24,
        txLimit: 1000
    }
};

export const getters = {
    getActiveItem() {
        return state.addresses.find(a => a.address === state.activeAddress);
    }
};

export const actions = {
    addOrUpdateAddress(address, statusOrPatch) {
        const patch = typeof statusOrPatch === 'string'
            ? { status: statusOrPatch }
            : (statusOrPatch || {});
        const existing = state.addresses.find(a => a.address === address);

        if (existing) {
            Object.assign(existing, patch);
        } else {
            state.addresses.unshift({
                address,
                status: 'pending',
                jobId: null,
                totalTransactions: 0,
                processedTransactions: 0,
                remainingTransactions: 0,
                updatedAt: null,
                ...patch
            });
        }
        
        if (!state.activeAddress || !existing) {
            state.activeAddress = address;
        }
    },
    removeAddress(address) {
        const intervalId = state.pollingIntervals[address];
        if (intervalId) {
            clearInterval(intervalId);
            delete state.pollingIntervals[address];
        }

        state.addresses = state.addresses.filter(a => a.address !== address);
        if (state.activeAddress === address) {
            state.activeAddress = state.addresses.length > 0 ? state.addresses[0].address : null;
        }
    },
    setActiveAddress(address) {
        state.activeAddress = address;
    },
    setLoading(loading) {
        state.isLoading = loading;
    },
    updateFilter(key, value) {
        state.filters[key] = value;
    },
    setPollingInterval(address, intervalId) {
        state.pollingIntervals[address] = intervalId;
    },
    clearPollingInterval(address) {
        const intervalId = state.pollingIntervals[address];
        if (intervalId) {
            clearInterval(intervalId);
            delete state.pollingIntervals[address];
        }
    }
};
